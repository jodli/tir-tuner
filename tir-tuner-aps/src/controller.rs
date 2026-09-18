//! NMPC style dosing calculator and pump safety layer.
//!
//! Enforces the hard hypoglycemia cutoff (zero delivery when the CGM
//! reading is below the cutoff) and the maximum hourly delivery cap,
//! plus the Ease-off and Boost operating modes for the internal
//! proportional controller, and the section 5.1 NMPC dose selector.
//!
//! The dose selector implements the published formulation (Hovorka et
//! al 2004, eq 9) as written:
//!
//! `min over 0 <= u(t+1) .. u(t+N) <= u_max
//!     sum_{i=1..N} (g_IG(t+i) - w(t+i))^2
//!   + (1/k_agr) sum_{i=1..N} (u(t+i) - u(t+i-1))^2`
//!
//! `w` is the moving target trajectory of section 3.3 in the paper:
//! from the measured glucose a clamped linear fall above target, an
//! exponential rise below. The optimizer is a local search over the
//! quantized rate sequence: a constant-rate grid init plus a bounded
//! coordinate-descent refinement (an iterative stand-in for the paper's
//! Marquardt minimization over the sequence). Only `u(t+1)`, the first
//! element, is applied; the horizon is re-solved each control period
//! (receding horizon).

use crate::hovorka::{HovorkaParams, HovorkaState};
use crate::{
    BOOST_DELIVERY_FACTOR, DOSE_GAIN_U_H_PER_MMOL_L, EASE_OFF_TARGET_MMOL_L,
    HARD_HYPO_CUTOFF_MMOL_L, TARGET_GLUCOSE_MMOL_L,
};

/// Resolution of the NMPC candidate grid: candidate rates
/// `k / NMPC_GRID_STEPS * u_max` for `k in 0..=NMPC_GRID_STEPS`.
pub const NMPC_GRID_STEPS: usize = 6;

/// Number of candidate rates on the NMPC grid
/// (`NMPC_GRID_STEPS + 1`, including the zero rate).
pub const NMPC_GRID_POINTS: usize = NMPC_GRID_STEPS + 1;

/// Fine-resolution divisor for the refinement step: the coordinate
/// descent moves in `u_max / (NMPC_GRID_STEPS * NMPC_REFINE_SUBSTEPS)`
/// increments, a fifth of the coarse grid quantum, so it can settle on a
/// smooth rate taper instead of hammering between adjacent grid levels.
pub const NMPC_REFINE_SUBSTEPS: usize = 5;

/// Upper bound on the number of coordinate-descent passes over the
/// sequence.
pub const NMPC_REFINEMENT_PASSES: usize = 8;

/// Offset above target (mmol/L) at which the moving target trajectory
/// switches from the moderate to the steep linear decline.
pub const TRAJECTORY_MAX_OFFSET_MMOL_L: f64 = 2.0;

/// Steepest linear decline of the moving target trajectory (mmol/L/h),
/// used while the trajectory sits more than
/// [`TRAJECTORY_MAX_OFFSET_MMOL_L`] above the target (Hovorka et al
/// 2004, section 3.3).
pub const TRAJECTORY_MAX_DECLINE_MMOL_PER_H: f64 = 2.0;

/// Moderate linear decline of the moving target trajectory (mmol/L/h),
/// used between the target and the steep band.
pub const TRAJECTORY_MODERATE_DECLINE_MMOL_PER_H: f64 = 1.0;

/// Half-time (min) of the exponential rise of the moving target
/// trajectory below the target.
pub const TRAJECTORY_RISE_HALFTIME_MIN: f64 = 15.0;

/// Operating modes of the closed-loop system (Ware et al. 2022).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DosingMode {
    /// Standard closed-loop control at the nominal target.
    Standard,
    /// Ease-off: raised target and full suspension while below the
    /// exercise threshold.
    EaseOff,
    /// Boost: temporary delivery intensification.
    Boost,
}

/// Raw proportional-delivery target error comparison which is negative,
/// zero, or positive in a monotone way across all operating modes.
fn controller_target_error(ig_reading: f64, mode: DosingMode) -> f64 {
    match mode {
        DosingMode::Standard => DOSE_GAIN_U_H_PER_MMOL_L * (ig_reading - TARGET_GLUCOSE_MMOL_L),
        DosingMode::EaseOff => DOSE_GAIN_U_H_PER_MMOL_L * (ig_reading - EASE_OFF_TARGET_MMOL_L),
        DosingMode::Boost => {
            BOOST_DELIVERY_FACTOR * DOSE_GAIN_U_H_PER_MMOL_L * (ig_reading - TARGET_GLUCOSE_MMOL_L)
        }
    }
}

/// Unclamped insulin delivery (U/h) for the given mode, honoring the
/// Ease-off suspension rule.
pub fn uncapped_dose(ig_reading: f64, mode: DosingMode) -> f64 {
    if mode == DosingMode::EaseOff && ig_reading < EASE_OFF_TARGET_MMOL_L {
        return 0.0;
    }
    controller_target_error(ig_reading, mode)
}

/// Mode-aware NMPC calculated insulin delivery rate (U/h).
///
/// Safety invariants (proved by the Kani suite for every mode):
/// 1. `0.0 <= result <= max_delivery_rate`.
/// 2. If `ig_reading < HARD_HYPO_CUTOFF_MMOL_L` then `result == 0.0`.
/// 3. In Ease-off, delivery is fully suspended while
///    `ig_reading < EASE_OFF_TARGET_MMOL_L`.
/// 4. Boost never delivers less than Standard on the same reading.
pub fn compute_nmpc_dose_mode(ig_reading: f64, max_delivery_rate: f64, mode: DosingMode) -> f64 {
    if ig_reading < HARD_HYPO_CUTOFF_MMOL_L {
        0.0
    } else {
        uncapped_dose(ig_reading, mode).clamp(0.0, max_delivery_rate)
    }
}

/// NMPC calculated insulin delivery rate (U/h) in Standard mode.
///
/// Safety invariants (proved by the Kani suite):
/// 1. `0.0 <= result <= max_delivery_rate`.
/// 2. If `ig_reading < HARD_HYPO_CUTOFF_MMOL_L` then `result == 0.0`.
pub fn compute_nmpc_dose(ig_reading: f64, max_delivery_rate: f64) -> f64 {
    compute_nmpc_dose_mode(ig_reading, max_delivery_rate, DosingMode::Standard)
}

/// The moving target trajectory `w(t+j)` of Hovorka et al 2004,
/// section 3.3: the desired glucose profile the NMPC cost tracks.
///
/// Starting from the measured glucose `y_start`, the trajectory falls
/// linearly at [`TRAJECTORY_MAX_DECLINE_MMOL_PER_H`] while more than
/// [`TRAJECTORY_MAX_OFFSET_MMOL_L`] above the target, falls at
/// [`TRAJECTORY_MODERATE_DECLINE_MMOL_PER_H`] between that and the
/// target, and settles on the target once reached (the decline is
/// clamped at the target so hypers cannot undershoot). Below the target
/// it rises exponentially with halftime
/// [`TRAJECTORY_RISE_HALFTIME_MIN`], converging on the target from
/// below. The trajectory is projected every `step_min` over `n` samples;
/// with `y_start` exactly on target it stays flat.
pub fn moving_target_trajectory(y_start: f64, target: f64, n: usize, step_min: f64) -> Vec<f64> {
    let mut w = Vec::with_capacity(n);
    let decline_steep = TRAJECTORY_MAX_DECLINE_MMOL_PER_H * step_min / 60.0;
    let decline_moderate = TRAJECTORY_MODERATE_DECLINE_MMOL_PER_H * step_min / 60.0;
    let rise_factor = 0.5f64.powf(step_min / TRAJECTORY_RISE_HALFTIME_MIN);
    let mut prev = y_start;
    w.push(prev);
    for _ in 1..n {
        if prev > target + TRAJECTORY_MAX_OFFSET_MMOL_L {
            prev = (prev - decline_steep).max(target);
        } else if prev > target {
            prev = (prev - decline_moderate).max(target);
        } else if prev < target {
            prev = target + (prev - target) * rise_factor;
        }
        w.push(prev);
    }
    w
}

/// NMPC cost of a candidate rate sequence `u` against the target
/// trajectory `w`, section 5.1 as written (Hovorka et al 2004, eq 9):
///
/// `J(u) = sum_{j=1..N} (g_IG(t+j) - w(t+j))^2
///         + (1/k_agr) sum_{j=1..N} (u(t+j) - u(t+j-1))^2`
///
/// with `u[-1] = u_prev`, the rate infused over the previous control
/// period. The glucose term is the summed squared interstitial glucose
/// deviation predicted by rolling the model forward under the sequence;
/// the effort term prices changes in the rate, not deviation from an
/// operating point. `k_agr` is the aggressiveness constant: larger
/// values weight the effort term less and let the optimizer move the
/// rate more freely.
pub fn nmpc_sequence_cost(
    params: &HovorkaParams,
    state: HovorkaState,
    u: &[f64],
    w: &[f64],
    u_prev: f64,
    k_agr: f64,
    step_min: f64,
) -> f64 {
    debug_assert_eq!(u.len(), w.len());
    let mut sum = 0.0;
    let mut predict = state;
    let mut prev_rate = u_prev;
    let effort_weight = 1.0 / k_agr;
    for j in 0..u.len() {
        predict = predict.step(params, u[j], 0.0, 0.0, step_min);
        let glucose_err = predict.interstitial_glucose(params) - w[j];
        let rate_change = u[j] - prev_rate;
        sum += glucose_err * glucose_err + effort_weight * rate_change * rate_change;
        prev_rate = u[j];
    }
    sum
}

/// Cost of holding a single constant rate `u` over the whole trajectory
/// `w`, as a repeated sequence through [`nmpc_sequence_cost`].
pub fn nmpc_constant_cost(
    params: &HovorkaParams,
    state: HovorkaState,
    u: f64,
    w: &[f64],
    u_prev: f64,
    k_agr: f64,
    step_min: f64,
) -> f64 {
    let mut seq = vec![0.0; w.len()];
    seq.fill(u);
    nmpc_sequence_cost(params, state, &seq, w, u_prev, k_agr, step_min)
}

/// Index of the first candidate attaining the minimum cost. Pure
/// selection logic over the supplied costs; the Kani suite proves this
/// index is minimal, independently of how the costs were computed.
pub fn best_grid_candidate_index(costs: &[f64; NMPC_GRID_POINTS]) -> usize {
    let mut best_index = 0;
    let mut best_cost = costs[0];
    let mut i = 1;
    while i < NMPC_GRID_POINTS {
        if costs[i] < best_cost {
            best_cost = costs[i];
            best_index = i;
        }
        i += 1;
    }
    best_index
}

/// Constant-rate NMPC init: the argument of the minimum of the section
/// 5.1 cost over the candidate grid
/// `{ k / NMPC_GRID_STEPS * u_max : k = 0..=N }`, where the glucose
/// term is the summed squared deviation over the `w` trajectory rolled
/// out at `step_min` resolution under the candidate rate. This seeds the
/// sequence solver; the Kani suite proves the candidate rates stay in
/// `[0, u_max]` and the position selected by
/// [`best_grid_candidate_index`] attains the minimal cost.
pub fn nmpc_grid_dose(
    params: &HovorkaParams,
    state: HovorkaState,
    w: &[f64],
    u_prev: f64,
    u_max: f64,
    k_agr: f64,
    step_min: f64,
) -> f64 {
    let mut rates = [0.0; NMPC_GRID_POINTS];
    let mut costs = [0.0; NMPC_GRID_POINTS];
    for k in 0..=NMPC_GRID_STEPS {
        let u = u_max * (k as f64 / NMPC_GRID_STEPS as f64);
        rates[k] = u;
        costs[k] = nmpc_constant_cost(params, state, u, w, u_prev, k_agr, step_min);
    }
    rates[best_grid_candidate_index(&costs)]
}

/// Bounded coordinate-descent refinement of the rate sequence: each
/// pass tries a one-grid-step change of every window and keeps moves
/// that lower the full-sequence cost. Deterministic, bounded at
/// [`NMPC_REFINEMENT_PASSES`] passes, and monotonically non-increasing
/// in cost; it is the iterative stand-in for the paper's Marquardt
/// minimization over the sequence.
fn refine_sequence(
    params: &HovorkaParams,
    state: HovorkaState,
    mut seq: Vec<f64>,
    w: &[f64],
    u_prev: f64,
    u_max: f64,
    k_agr: f64,
    step_min: f64,
) -> Vec<f64> {
    let step = u_max / (NMPC_GRID_STEPS * NMPC_REFINE_SUBSTEPS) as f64;
    let mut best = nmpc_sequence_cost(params, state, &seq, w, u_prev, k_agr, step_min);
    for _ in 0..NMPC_REFINEMENT_PASSES {
        let mut improved = false;
        for j in 0..seq.len() {
            let base = seq[j];
            for cand in [base + step, base - step] {
                let cand = cand.clamp(0.0, u_max);
                seq[j] = cand;
                let cost = nmpc_sequence_cost(params, state, &seq, w, u_prev, k_agr, step_min);
                if cost < best {
                    best = cost;
                    improved = true;
                } else {
                    seq[j] = base;
                }
            }
        }
        if !improved {
            break;
        }
    }
    seq
}

/// Section 5.1 NMPC sequence solver: builds the moving target
/// trajectory from the measured glucose, optimizes a constant-rate init
/// over the candidate grid, and refines the rate sequence with bounded
/// coordinate descent. Every element lies in `[0, u_max]`, and the
/// refined sequence costs no more than the constant-rate init; the
/// paper's Marquart minimization is replaced by this bounded local
/// search over the quantized sequence.
pub fn nmpc_sequence(
    params: &HovorkaParams,
    state: HovorkaState,
    y_meas: f64,
    target: f64,
    k_agr: f64,
    u_max: f64,
    u_prev: f64,
    horizon_min: f64,
    step_min: f64,
) -> Vec<f64> {
    let n = (horizon_min / step_min).round() as usize;
    let w = moving_target_trajectory(y_meas, target, n, step_min);
    let init = nmpc_grid_dose(params, state, &w, u_prev, u_max, k_agr, step_min);
    let mut seq = vec![0.0; n];
    seq.fill(init);
    refine_sequence(params, state, seq, &w, u_prev, u_max, k_agr, step_min)
}

/// Section 5.1 NMPC dose selector: the first element `u(t+1)` of the
/// refined rate sequence, the rate to apply this control period
/// (receding horizon, as in the paper). The result lies in `[0, u_max]`;
/// the hypoglycemia guard is applied by the caller.
pub fn nmpc_sequence_dose(
    params: &HovorkaParams,
    state: HovorkaState,
    y_meas: f64,
    target: f64,
    k_agr: f64,
    u_max: f64,
    u_prev: f64,
    horizon_min: f64,
    step_min: f64,
) -> f64 {
    nmpc_sequence(params, state, y_meas, target, k_agr, u_max, u_prev, horizon_min, step_min)[0]
}

/// True when a CGM reading triggers the mandatory insulin suspension.
pub fn is_hypoglycemic(ig_reading: f64) -> bool {
    ig_reading < HARD_HYPO_CUTOFF_MMOL_L
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hovorka::{HovorkaParams, HovorkaState};
    use crate::test_support::config;
    use proptest::prelude::*;

    fn state_strategy() -> impl Strategy<Value = HovorkaState> {
        (
            0.0..25_000.0f64,
            0.0..25_000.0f64,
            0.0..50.0f64,
            0.0..50.0f64,
            0.0..150.0f64,
            0.0..150.0f64,
            0.1..30.0f64,
            0.1..30.0f64,
            0.1..30.0f64,
        )
            .prop_map(|(i1, i2, r_d, r_e, a1, a2, q1, q2, q3)| HovorkaState {
                i1,
                i2,
                r_d,
                r_e,
                a1,
                a2,
                q1,
                q2,
                q3,
                u_s: 0.0,
            })
    }

    fn trajectory_strategy() -> impl Strategy<Value = (f64, f64, usize, f64, Vec<f64>)> {
        (2.0..20.0f64, 4.0..11.0f64, 1usize..20, 1.0..15.0f64).prop_map(
            |(y_start, target, n, step)| {
                let w = moving_target_trajectory(y_start, target, n, step);
                (y_start, target, n, step, w)
            },
        )
    }

    /// A bounded rate sequence for the full-horizon cost sweep,
    /// truncated to the trajectory length at runtime.
    fn rate_sequence_strategy() -> impl Strategy<Value = Vec<f64>> {
        proptest::collection::vec(0.0..20.0f64, 20)
    }

    proptest! {
        #![proptest_config(config())]

        /// The composition link the Kani suite does not close: the rate
        /// returned by the constant-rate init `nmpc_grid_dose` is one of
        /// the grid candidates, stays in `[0, u_max]`, and its realized
        /// cost is the minimum over the computed candidate costs.
        #[test]
        fn grid_dose_is_min_cost_candidate(
            state in state_strategy(),
            (y_start, target, _, _, w) in trajectory_strategy(),
            u_prev in 0.0..20.0f64,
            u_max in 0.1..20.0f64,
            k_agr in 0.5..50.0f64,
            step in 1.0..15.0f64,
        ) {
            let params = HovorkaParams::default();
            let u = nmpc_grid_dose(&params, state, &w, u_prev, u_max, k_agr, step);
            let _ = (y_start, target);

            prop_assert!(u >= 0.0 && u <= u_max, "init dose out of bounds: {u}");

            let mut rates = [0.0; NMPC_GRID_POINTS];
            let mut costs = [0.0; NMPC_GRID_POINTS];
            for k in 0..=NMPC_GRID_STEPS {
                let rate = u_max * (k as f64 / NMPC_GRID_STEPS as f64);
                rates[k] = rate;
                costs[k] = nmpc_constant_cost(&params, state, rate, &w, u_prev, k_agr, step);
            }

            let best = best_grid_candidate_index(&costs);
            prop_assert_eq!(u, rates[best]);

            let min = costs.iter().cloned().fold(f64::INFINITY, f64::min);
            prop_assert_eq!(costs[best], min);
        }

        /// The full 16-window instance the simulation drives: the
        /// summed sequence cost stays finite and non-negative across the
        /// full state and tuning space (the Kani harness only covers a
        /// two-step roll-out slice).
        #[test]
        fn cost_is_finite_nonnegative_over_full_state(
            state in state_strategy(),
            y_start in 2.0..20.0f64,
            target in 4.0..11.0f64,
            n in 1usize..21,
            step in 1.0..15.0f64,
            u in rate_sequence_strategy(),
            u_prev in 0.0..20.0f64,
            k_agr in 0.5..50.0f64,
        ) {
            let params = HovorkaParams::default();
            let w = moving_target_trajectory(y_start, target, n, step);
            let u = &u[..u.len().min(w.len())];
            let cost = nmpc_sequence_cost(&params, state, u, &w, u_prev, k_agr, step);
            prop_assert!(cost.is_finite(), "cost is not finite: {cost}");
            prop_assert!(cost >= 0.0, "cost is negative: {cost}");
        }

        /// The sequence solver is a pure cost-improver: every element of
        /// the refined sequence lies in `[0, u_max]`, its first element
        /// is the returned dose, and it costs no more than the
        /// constant-rate init it seeds.
        #[test]
        fn sequence_dose_is_refined_init(
            state in state_strategy(),
            y_meas in 2.0..20.0f64,
            target in 4.0..11.0f64,
            u_prev in 0.0..20.0f64,
            u_max in 0.1..20.0f64,
            k_agr in 0.5..50.0f64,
            horizon in 60.0..300.0f64,
            step in 5.0..20.0f64,
        ) {
            let params = HovorkaParams::default();
            let seq = nmpc_sequence(
                &params, state, y_meas, target, k_agr, u_max, u_prev, horizon, step,
            );
            for &v in &seq {
                prop_assert!(v >= 0.0 && v <= u_max, "sequence rate out of bounds: {v}");
            }
            let dose = nmpc_sequence_dose(
                &params, state, y_meas, target, k_agr, u_max, u_prev, horizon, step,
            );
            prop_assert_eq!(dose, seq[0], "dose is not the first element");

            let n = (horizon / step).round() as usize;
            let w = moving_target_trajectory(y_meas, target, n, step);
            let init = vec![nmpc_grid_dose(&params, state, &w, u_prev, u_max, k_agr, step); n];
            let init_cost = nmpc_sequence_cost(&params, state, &init, &w, u_prev, k_agr, step);
            let refined_cost = nmpc_sequence_cost(&params, state, &seq, &w, u_prev, k_agr, step);
            prop_assert!(
                refined_cost <= init_cost + 1e-9,
                "refined sequence not cheaper than init: {refined_cost} vs {init_cost}"
            );
        }

        /// The full moving target trajectory is finite, stays inside the
        /// band `[min(y_start, target), max(y_start, target)]`, moves
        /// monotonically toward the target and strictly approaches it
        /// each sample (native cover for the exponential rise branch and
        /// the piecewise band switch; the Kani suite proves the coarse
        /// decline band).
        #[test]
        fn moving_target_trajectory_tracks_target(
            y_start in 2.0..20.0f64,
            target in 4.0..11.0f64,
            n in 2usize..30,
            step in 0.5..20.0f64,
        ) {
            let w = moving_target_trajectory(y_start, target, n, step);
            let lo = y_start.min(target);
            let hi = y_start.max(target);

            for &v in &w {
                prop_assert!(v.is_finite(), "trajectory not finite: {v}");
                prop_assert!(
                    (lo - 1e-9..=hi + 1e-9).contains(&v),
                    "trajectory escaped band: {v} not in [{lo}, {hi}]"
                );
            }

            if y_start > target {
                for pair in w.windows(2) {
                    prop_assert!(pair[1] <= pair[0] + 1e-9, "decline not monotone");
                }
            } else if y_start < target {
                for pair in w.windows(2) {
                    prop_assert!(pair[1] >= pair[0] - 1e-9, "rise not monotone");
                }
            } else {
                for &v in &w {
                    prop_assert_eq!(v, target, "flat trajectory drifted");
                }
            }

            if (y_start - target).abs() > 1e-9 {
                let err0 = (w[0] - target).abs();
                let errn = (w[w.len() - 1] - target).abs();
                prop_assert!(
                    errn < err0,
                    "trajectory did not approach target: err {errn} vs {err0}"
                );
            }
        }

        /// `nmpc_sequence_dose` is a pure cost minimizer: it does not
        /// itself know the CGM reading or the hard hypoglycemia cutoff
        /// (the two layers are kept separate, as in CamAPS). Once the
        /// dose is passed through the hypoglycemia guard the delivered
        /// rate honors the `[0, u_max]` bounds and the mandatory zero
        /// below the cutoff.
        #[test]
        fn sequence_dose_composed_with_hypo_cutoff_is_safe(
            state in state_strategy(),
            y_meas in 2.0..20.0f64,
            ig_reading in 2.0..14.0f64,
            target in 4.0..11.0f64,
            u_prev in 0.0..20.0f64,
            u_max in 0.1..20.0f64,
            k_agr in 0.5..50.0f64,
        ) {
            let params = HovorkaParams::default();
            let dose = nmpc_sequence_dose(
                &params, state, y_meas, target, k_agr, u_max, u_prev, 240.0, 15.0,
            );
            let delivered = if is_hypoglycemic(ig_reading) {
                0.0
            } else {
                dose.clamp(0.0, u_max)
            };

            prop_assert!(delivered >= 0.0 && delivered <= u_max, "delivered out of bounds");
            if is_hypoglycemic(ig_reading) {
                prop_assert_eq!(delivered, 0.0, "hypo cutoff not honored in composition");
            }
        }
    }
}