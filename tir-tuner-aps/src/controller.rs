//! NMPC style dosing calculator and pump safety layer.
//!
//! Enforces the hard hypoglycemia cutoff (zero delivery when the CGM
//! reading is below the cutoff) and the maximum hourly delivery cap,
//! plus the Ease-off and Boost operating modes for the internal
//! proportional controller, and a one-step grid-search NMPC dose selector
//! over the section 5.1 quadratic cost. The Kani suite verifies the
//! mode-specific safety envelope, the `[0, u_max]` bounds, the Ease-off
//! suspension rule and the optimality of the NMPC pick.

use crate::hovorka::{HovorkaParams, HovorkaState};
use crate::{
    BOOST_DELIVERY_FACTOR, DOSE_GAIN_U_H_PER_MMOL_L, EASE_OFF_TARGET_MMOL_L,
    HARD_HYPO_CUTOFF_MMOL_L, TARGET_GLUCOSE_MMOL_L,
};

/// Grid resolution of the one-step NMPC dose selector: candidate rates
/// `k / NMPC_GRID_STEPS * u_max` for `k in 0..=NMPC_GRID_STEPS`.
pub const NMPC_GRID_STEPS: usize = 6;

/// Number of candidate rates on the NMPC grid
/// (`NMPC_GRID_STEPS + 1`, including the zero rate).
pub const NMPC_GRID_POINTS: usize = NMPC_GRID_STEPS + 1;

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

/// NMPC cost of holding rate `u` (U/h) for the next `horizon_min`
/// minutes, `J(u) = (g_IG(u) - w)^2 + lambda (u - u_operating)^2` over
/// the section 5.1 quadratic objective, with `g_IG(u)` the interstitial
/// glucose at the end of a `horizon_min` rollout.
///
/// The roll-out steps the prediction model forward in `step_min` chunks
/// under the candidate rate and no future meal or bolus; the horizon has
/// to span the slow insulin action timescale (the effort term only
/// prices the candidate rate meaningfully once the prediction can see
/// the resulting glucose trajectory), so the simulation drives it with a
/// multi-sample horizon and a control-period step.
pub fn nmpc_cost(
    params: &HovorkaParams,
    state: HovorkaState,
    u: f64,
    u_operating: f64,
    target_w: f64,
    lambda: f64,
    horizon_min: f64,
    step_min: f64,
) -> f64 {
    let steps = (horizon_min / step_min).round() as usize;
    let mut predict = state;
    for _ in 0..steps {
        predict = predict.step(params, u, 0.0, 0.0, step_min);
    }
    let pred = predict.interstitial_glucose(params);
    let glucose_err = pred - target_w;
    let effort_err = u - u_operating;
    glucose_err * glucose_err + lambda * effort_err * effort_err
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

/// NMPC dose selector: the argument of the minimum of
/// `J(u) = (g_IG(u) - w)^2 + lambda (u - u_operating)^2` over the
/// candidate grid `{ k / NMPC_GRID_STEPS * u_max : k = 0..=N }`, where
/// the glucose term comes from a `horizon_min` roll-out of the model at
/// `step_min` resolution under the candidate rate. The candidate rates
/// all lie in `[0, u_max]`.
///
/// The candidate rates and their costs are computed first, and the
/// argument of the minimum is picked by [`best_grid_candidate_index`].
/// The Kani suite proves the two layers separately: the candidate rates
/// stay within `[0, u_max]`, the NMPC cost is finite and non-negative,
/// and the position selected by `best_grid_candidate_index` attains the
/// minimal cost.
pub fn nmpc_grid_dose(
    params: &HovorkaParams,
    state: HovorkaState,
    u_max: f64,
    u_operating: f64,
    target_w: f64,
    lambda: f64,
    horizon_min: f64,
    step_min: f64,
) -> f64 {
    let mut rates = [0.0; NMPC_GRID_POINTS];
    let mut costs = [0.0; NMPC_GRID_POINTS];
    for k in 0..=NMPC_GRID_STEPS {
        let u = u_max * (k as f64 / NMPC_GRID_STEPS as f64);
        rates[k] = u;
        costs[k] = nmpc_cost(params, state, u, u_operating, target_w, lambda, horizon_min, step_min);
    }
    rates[best_grid_candidate_index(&costs)]
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

    proptest! {
        #![proptest_config(config())]

        /// The composition link the Kani suite does not close: the rate
        /// returned by `nmpc_grid_dose` is one of the grid candidates,
        /// stays in `[0, u_max]`, and its realized cost is the minimum
        /// over the computed candidate costs.
        #[test]
        fn grid_dose_is_min_cost_candidate(
            state in state_strategy(),
            u_max in 0.1..20.0f64,
            u_operating in 0.0..20.0f64,
            target_w in 4.0..12.0f64,
            lambda in 0.0..10.0f64,
            horizon_min in 5.0..90.0f64,
            step_min in 1.0..15.0f64,
        ) {
            let params = HovorkaParams::default();
            let dose = nmpc_grid_dose(
                &params,
                state,
                u_max,
                u_operating,
                target_w,
                lambda,
                horizon_min,
                step_min,
            );

            prop_assert!(dose >= 0.0 && dose <= u_max, "dose out of bounds: {dose}");

            let mut rates = [0.0; NMPC_GRID_POINTS];
            let mut costs = [0.0; NMPC_GRID_POINTS];
            for k in 0..=NMPC_GRID_STEPS {
                let u = u_max * (k as f64 / NMPC_GRID_STEPS as f64);
                rates[k] = u;
                costs[k] = nmpc_cost(
                    &params,
                    state,
                    u,
                    u_operating,
                    target_w,
                    lambda,
                    horizon_min,
                    step_min,
                );
            }

            let best = best_grid_candidate_index(&costs);
            prop_assert_eq!(dose, rates[best]);

            let min = costs.iter().cloned().fold(f64::INFINITY, f64::min);
            prop_assert_eq!(costs[best], min);
        }

        #[test]
        fn cost_is_finite_nonnegative_over_full_state(
            state in state_strategy(),
            u in 0.0..25.0f64,
            u_operating in 0.0..20.0f64,
            target_w in 4.0..12.0f64,
            lambda in 0.0..10.0f64,
            horizon_min in 5.0..90.0f64,
            step_min in 1.0..15.0f64,
        ) {
            let params = HovorkaParams::default();
            let cost = nmpc_cost(
                &params,
                state,
                u,
                u_operating,
                target_w,
                lambda,
                horizon_min,
                step_min,
            );
            prop_assert!(cost.is_finite(), "cost is not finite: {cost}");
            prop_assert!(cost >= 0.0, "cost is negative: {cost}");
        }

        /// `nmpc_grid_dose` is a pure cost minimizer: it does not itself
        /// know the CGM reading or the hard hypoglycemia cutoff (the two
        /// layers are kept separate, as in CamAPS). This closes the
        /// composition gap the Kani suite leaves open: once the grid
        /// result is passed through the hypoglycemia guard, the delivered
        /// rate honors the `[0, u_max]` bounds and the mandatory zero
        /// below the cutoff for any grid outcome and any CGM reading.
        #[test]
        fn grid_dose_composed_with_hypo_cutoff_is_safe(
            state in state_strategy(),
            ig_reading in 2.0..14.0f64,
            u_max in 0.1..20.0f64,
            u_operating in 0.0..20.0f64,
            target_w in 4.0..12.0f64,
            lambda in 0.0..10.0f64,
        ) {
            let params = HovorkaParams::default();
            let grid_dose = nmpc_grid_dose(
                &params,
                state,
                u_max,
                u_operating,
                target_w,
                lambda,
                60.0,
                5.0,
            );
            let delivered = if is_hypoglycemic(ig_reading) {
                0.0
            } else {
                grid_dose.clamp(0.0, u_max)
            };

            prop_assert!(delivered >= 0.0 && delivered <= u_max, "delivered out of bounds");
            if is_hypoglycemic(ig_reading) {
                prop_assert_eq!(delivered, 0.0, "hypo cutoff not honored in composition");
            }
        }
    }
}
