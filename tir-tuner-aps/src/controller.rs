//! NMPC dose selection (Hovorka 2004 eq 9) and the Auto mode rules applied
//! to its rate: Boost, Ease-off and the hypo cutoff.
//!
//! Every period the controller predicts plasma glucose over the horizon for
//! candidate rate sequences, picks the sequence with the lowest eq 9 cost
//! and applies its first rate. H04 minimizes with Marquardt; here a grid
//! search seeds a coordinate descent, our own stand-in.

use crate::hovorka::{HovorkaParams, HovorkaState};
use tir_tuner_common::settings::AUTO_TARGET_MMOL_L;
use tir_tuner_common::units::MIN_PER_HOUR;

/// Control period (min): one CGM reading, one rate. src: h04-period
pub const CONTROL_PERIOD_MIN: f64 = 15.0;

/// Prediction horizon N (min). src: h04-horizon
pub const HORIZON_MIN: f64 = 240.0;

/// Maximum rate (U/h). CamAPS sets it per person and doesn't publish how.
/// src: h04-max-rate
pub const MAX_RATE_U_PER_H: f64 = 4.0;

/// Aggressiveness `k_agr`. H04 publishes the tuning rule, not the value.
/// src: own-kagr
pub const K_AGR: f64 = 5.0;

/// Euler step of the prediction (min). The patent discretizes exactly;
/// 1 min keeps `k31 dt` well below 1. src: own-rollout-step
pub const ROLLOUT_STEP_MIN: f64 = 1.0;

/// Trajectory bands: 2 mmol/L/h more than this above target, 1 mmol/L/h
/// below it. H04 states them for target 6 as 8 and 6 mmol/L.
/// src: h04-trajectory
pub const TRAJECTORY_STEEP_ABOVE_MMOL_L: f64 = 2.0;
/// src: h04-trajectory
pub const TRAJECTORY_STEEP_MMOL_L_PER_H: f64 = 2.0;
/// src: h04-trajectory
pub const TRAJECTORY_MODERATE_MMOL_L_PER_H: f64 = 1.0;
/// Below target the trajectory rises with this halftime (min).
/// src: h04-trajectory
pub const TRAJECTORY_RISE_HALFTIME_MIN: f64 = 15.0;

/// Grid of the constant-rate seed: `u_max k / GRID_STEPS`. src: own-optimizer
pub const GRID_STEPS: usize = 6;
/// The descent moves in `u_max / (GRID_STEPS * REFINE_SUBSTEPS)`.
/// src: own-optimizer
pub const REFINE_SUBSTEPS: usize = 5;
/// Descent passes at most, H04's Marquardt iteration cap. src: own-optimizer
pub const REFINE_PASSES: usize = 50;
/// Candidates on the seed grid, zero included.
pub const GRID_POINTS: usize = GRID_STEPS + 1;

/// No insulin below this reading (mmol/L): the lowest personal target. No
/// source has a threshold; the manual says insulin may be reduced further
/// when glucose is low or falling. src: own-hypo-cutoff
pub const HYPO_CUTOFF_MMOL_L: f64 = AUTO_TARGET_MMOL_L.0;

/// Ease-off suspends insulin below this reading (mmol/L). Its raised
/// target isn't published; we use this value, so the target never sits
/// below the suspension. src: ware2022-ease-off
pub const EASE_OFF_SUSPEND_MMOL_L: f64 = 7.0;

/// Boost multiplies the algorithm's rate by this while above target.
/// src: ware2022b-boost
pub const BOOST_FACTOR: f64 = 1.35;

/// User-selected Auto mode variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Plain Auto mode.
    Standard,
    /// Raised target, suspension below [`EASE_OFF_SUSPEND_MMOL_L`].
    EaseOff,
    /// About 35% more insulin until glucose reaches the target.
    Boost,
}

impl Mode {
    /// The target the NMPC aims at in this mode (mmol/L). Ease-off's raised
    /// target is ours. src: own-ease-off-target
    pub fn target(self, target: f64) -> f64 {
        match self {
            Mode::EaseOff => target.max(EASE_OFF_SUSPEND_MMOL_L),
            Mode::Standard | Mode::Boost => target,
        }
    }

    /// No algorithm insulin at this reading.
    pub fn suspended(self, reading: f64) -> bool {
        reading < HYPO_CUTOFF_MMOL_L || (self == Mode::EaseOff && reading < EASE_OFF_SUSPEND_MMOL_L)
    }

    /// The rate delivered (U/h) for the NMPC rate `nmpc` at `reading`.
    /// Invariants: `verify_mode_rate_invariants`.
    pub fn rate(self, nmpc: f64, reading: f64, target: f64, u_max: f64) -> f64 {
        if self.suspended(reading) {
            return 0.0;
        }
        let boosted = self == Mode::Boost && reading > target;
        let rate = if boosted { nmpc * BOOST_FACTOR } else { nmpc };
        rate.clamp(0.0, u_max)
    }
}

/// The moving target trajectory, H04 section 3.3 and Fig 4: `w[0]` is
/// glucose now, `w[j]` the target at `t + j step_min`.
pub fn trajectory(y: f64, target: f64, n: usize, step_min: f64) -> Vec<f64> {
    let per_h = step_min / MIN_PER_HOUR;
    let rise = (-std::f64::consts::LN_2 * step_min / TRAJECTORY_RISE_HALFTIME_MIN).exp();
    let mut w = Vec::with_capacity(n + 1);
    let mut g = y;
    w.push(g);
    for _ in 0..n {
        g = if g > target + TRAJECTORY_STEEP_ABOVE_MMOL_L {
            (g - TRAJECTORY_STEEP_MMOL_L_PER_H * per_h).max(target)
        } else if g > target {
            (g - TRAJECTORY_MODERATE_MMOL_L_PER_H * per_h).max(target)
        } else {
            target + (g - target) * rise
        };
        w.push(g);
    }
    w
}

/// Index of the first minimum.
pub fn argmin(costs: &[f64; GRID_POINTS]) -> usize {
    let mut best = 0;
    for i in 1..GRID_POINTS {
        if costs[i] < costs[best] {
            best = i;
        }
    }
    best
}

/// NMPC settings.
#[derive(Clone, Copy, Debug)]
pub struct Nmpc {
    /// Aggressiveness `k_agr` of eq 9.
    pub k_agr: f64,
    /// Upper rate bound (U/h).
    pub u_max: f64,
    /// Sample spacing of the prediction (min), also the control period.
    pub step_min: f64,
    /// Samples in the horizon, N.
    pub n: usize,
}

impl Default for Nmpc {
    fn default() -> Self {
        Self {
            k_agr: K_AGR,
            u_max: MAX_RATE_U_PER_H,
            step_min: CONTROL_PERIOD_MIN,
            n: (HORIZON_MIN / CONTROL_PERIOD_MIN) as usize,
        }
    }
}

impl Nmpc {
    /// Eq 9 cost of rates `u` (one per sample, U/h) against trajectory `w`
    /// (`n + 1` points, `w[0]` now): squared plasma glucose error at
    /// `t + i`, plus rate changes from `u_prev` weighted `1 / k_agr`. No
    /// measurement arrives in the horizon, so `u_s` regresses (eq 57).
    pub fn cost(
        &self,
        params: &HovorkaParams,
        state: HovorkaState,
        u: &[f64],
        w: &[f64],
        u_prev: f64,
    ) -> f64 {
        debug_assert_eq!(u.len() + 1, w.len());
        let substeps = (self.step_min / ROLLOUT_STEP_MIN).round() as usize;
        let mut s = state;
        let mut prev = u_prev;
        let mut sum = 0.0;
        for (&rate, &target) in u.iter().zip(&w[1..]) {
            for _ in 0..substeps {
                s = s.step_unmeasured(params, rate, 0.0, ROLLOUT_STEP_MIN);
            }
            let err = s.plasma_glucose(params) - target;
            sum += err * err + (rate - prev) * (rate - prev) / self.k_agr;
            prev = rate;
        }
        sum
    }

    /// Candidate `k` of the seed grid (U/h).
    pub fn grid_rate(&self, k: usize) -> f64 {
        // Divide first: (u_max k) / N can round one ulp above u_max.
        self.u_max * (k as f64 / GRID_STEPS as f64)
    }

    /// Best constant rate on the seed grid, and its cost.
    pub fn seed(
        &self,
        params: &HovorkaParams,
        state: HovorkaState,
        w: &[f64],
        u_prev: f64,
    ) -> (f64, f64) {
        let costs: [f64; GRID_POINTS] = std::array::from_fn(|k| {
            self.cost(params, state, &vec![self.grid_rate(k); self.n], w, u_prev)
        });
        let k = argmin(&costs);
        (self.grid_rate(k), costs[k])
    }

    /// Rate sequence for this period: the seed, refined by coordinate
    /// descent, against a trajectory from the belief's plasma glucose (the
    /// patent tracks estimated plasma, eq 83). Every rate lies in
    /// `[0, u_max]` and the result costs no more than the seed.
    pub fn sequence(
        &self,
        params: &HovorkaParams,
        state: HovorkaState,
        target: f64,
        u_prev: f64,
    ) -> Vec<f64> {
        let w = trajectory(state.plasma_glucose(params), target, self.n, self.step_min);
        let (seed, mut best) = self.seed(params, state, &w, u_prev);
        let mut u = vec![seed; self.n];
        let delta = self.u_max / (GRID_STEPS * REFINE_SUBSTEPS) as f64;
        for _ in 0..REFINE_PASSES {
            let mut improved = false;
            for j in 0..self.n {
                let base = u[j];
                for cand in [base + delta, base - delta] {
                    u[j] = cand.clamp(0.0, self.u_max);
                    let c = self.cost(params, state, &u, &w, u_prev);
                    if c < best {
                        best = c;
                        improved = true;
                        break;
                    }
                    u[j] = base;
                }
            }
            if !improved {
                break;
            }
        }
        u
    }

    /// The first rate of [`Nmpc::sequence`]; H04 p7 discards the rest.
    pub fn dose(
        &self,
        params: &HovorkaParams,
        state: HovorkaState,
        target: f64,
        u_prev: f64,
    ) -> f64 {
        self.sequence(params, state, target, u_prev)[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::config;
    use proptest::prelude::*;

    fn params() -> HovorkaParams {
        HovorkaParams::from_weight_and_tdd(70.0, 50.0).resting_at(6.0)
    }

    /// H04 Fig 4: 2 mmol/L/h more than 2 above target, 1 mmol/L/h below,
    /// a 15 min halftime below target.
    #[test]
    fn h04_trajectory_bands() {
        let w = trajectory(10.0, 6.0, 1, 15.0);
        assert!((w[1] - 9.5).abs() < 1e-12);
        let w = trajectory(7.0, 6.0, 1, 15.0);
        assert!((w[1] - 6.75).abs() < 1e-12);
        let w = trajectory(4.0, 6.0, 2, 15.0);
        assert!((w[1] - 5.0).abs() < 1e-12);
        assert!((w[2] - 5.5).abs() < 1e-12);
        assert_eq!(trajectory(6.1, 6.0, 1, 15.0)[1], 6.0);
        // H04: 8 itself is in the 1 mmol/L/h band.
        assert!((trajectory(8.0, 6.0, 1, 15.0)[1] - 7.75).abs() < 1e-12);
    }

    #[test]
    fn argmin_takes_the_first_of_equal_costs() {
        assert_eq!(argmin(&[1.0; GRID_POINTS]), 0);
        let mut c = [2.0; GRID_POINTS];
        c[GRID_STEPS] = 1.0;
        assert_eq!(argmin(&c), GRID_STEPS);
    }

    #[test]
    fn grid_spans_zero_to_u_max() {
        let nmpc = Nmpc::default();
        assert_eq!(nmpc.grid_rate(0), 0.0);
        assert_eq!(nmpc.grid_rate(GRID_STEPS / 2), nmpc.u_max / 2.0);
        assert_eq!(nmpc.grid_rate(GRID_STEPS), nmpc.u_max);
    }

    /// Eq 9 compares the prediction at `t + i` with the target at `t + i`:
    /// a trajectory equal to the prediction costs nothing.
    #[test]
    fn h04_cost_matches_prediction_and_target_at_the_same_time() {
        let p = params();
        let nmpc = Nmpc::default();
        let start = HovorkaState::basal(&p, 9.0);
        let u = vec![p.bir; nmpc.n];
        let mut w = vec![start.plasma_glucose(&p)];
        let mut s = start;
        let substeps = (nmpc.step_min / ROLLOUT_STEP_MIN) as usize;
        for _ in 0..nmpc.n {
            for _ in 0..substeps {
                s = s.step(&p, p.bir, 0.0, ROLLOUT_STEP_MIN);
            }
            w.push(s.plasma_glucose(&p));
        }
        assert!(nmpc.cost(&p, start, &u, &w, p.bir) < 1e-20);
        // Same glucose, a step of 1 U/h from u_prev: the effort term alone.
        let effort = nmpc.cost(&p, start, &u, &w, p.bir - 1.0);
        assert!((effort - 1.0 / nmpc.k_agr).abs() < 1e-12, "{effort}");
    }

    /// Ware 2022b p3: about 35% more, only above target.
    #[test]
    fn ware2022b_boost_only_above_target() {
        assert!((Mode::Boost.rate(1.0, 8.0, 6.0, 4.0) - 1.35).abs() < 1e-12);
        assert_eq!(Mode::Boost.rate(1.0, 6.0, 6.0, 4.0), 1.0);
        assert_eq!(Mode::Boost.rate(3.5, 8.0, 6.0, 4.0), 4.0);
    }

    /// Ware 2022 p4: Ease-off suspends below 7.0 and raises the target.
    #[test]
    fn ware2022_ease_off_suspends_below_7() {
        assert_eq!(Mode::EaseOff.rate(1.0, 6.9, 6.0, 4.0), 0.0);
        assert_eq!(Mode::EaseOff.rate(1.0, 7.0, 6.0, 4.0), 1.0);
        assert_eq!(Mode::EaseOff.target(6.0), 7.0);
        assert_eq!(Mode::EaseOff.target(8.0), 8.0);
        assert_eq!(Mode::Standard.target(6.0), 6.0);
    }

    #[test]
    fn suspended_below_hypo_cutoff() {
        assert_eq!(Mode::Standard.rate(1.0, 4.39, 6.0, 4.0), 0.0);
        assert_eq!(Mode::Standard.rate(1.0, 4.4, 6.0, 4.0), 1.0);
        assert_eq!(Mode::Standard.rate(5.0, 9.0, 6.0, 4.0), 4.0);
    }

    /// More insulin when high, none needed when low.
    #[test]
    fn dose_follows_glucose() {
        let p = params();
        let nmpc = Nmpc::default();
        let high = nmpc.dose(&p, HovorkaState::basal(&p, 12.0), 6.0, p.bir);
        let low = nmpc.dose(&p, HovorkaState::basal(&p, 4.5), 6.0, p.bir);
        assert!(high > p.bir, "{high}");
        assert!(low < p.bir, "{low}");
    }

    /// High glucose: the seed reaches u_max, and the descent ends at a
    /// local minimum on its u_max/30 lattice, below the seed's cost.
    #[test]
    fn descent_converges_on_its_lattice() {
        let p = params();
        let nmpc = Nmpc::default();
        let s = HovorkaState::basal(&p, 9.0);
        let w = trajectory(9.0, 6.0, nmpc.n, nmpc.step_min);
        assert_eq!(
            nmpc.seed(&p, HovorkaState::basal(&p, 25.0), &w, p.bir).0,
            nmpc.u_max
        );
        let (_, seed_cost) = nmpc.seed(&p, s, &w, p.bir);
        let u = nmpc.sequence(&p, s, 6.0, p.bir);
        let best = nmpc.cost(&p, s, &u, &w, p.bir);
        assert!(best < seed_cost);
        let delta = nmpc.u_max / (GRID_STEPS * REFINE_SUBSTEPS) as f64;
        for j in 0..nmpc.n {
            let steps = u[j] / delta;
            assert!((steps - steps.round()).abs() < 1e-9, "u[{j}] = {}", u[j]);
            for d in [delta, -delta] {
                let mut v = u.clone();
                v[j] = (v[j] + d).clamp(0.0, nmpc.u_max);
                assert!(nmpc.cost(&p, s, &v, &w, p.bir) >= best);
            }
        }
    }

    /// Carbs in the gut raise the dose: the controller sees meals.
    #[test]
    fn dose_sees_carbs_in_the_gut() {
        let p = params();
        let nmpc = Nmpc::default();
        let fasting = HovorkaState::basal(&p, 7.0);
        let fed = HovorkaState {
            a1: 40.0,
            ..fasting
        };
        let d0 = nmpc.dose(&p, fasting, 6.0, p.bir);
        let d1 = nmpc.dose(&p, fed, 6.0, p.bir);
        assert!(d1 > d0, "{d1} <= {d0}");
    }

    proptest! {
        #![proptest_config(config())]

        /// Every rate in `[0, u_max]`, and the refined sequence costs no
        /// more than its seed.
        #[test]
        fn sequence_is_bounded_and_improves_on_the_seed(
            g in 3.0..20.0f64,
            target in AUTO_TARGET_MMOL_L.0..AUTO_TARGET_MMOL_L.1,
            u_prev in 0.0..4.0f64,
        ) {
            let p = params();
            let nmpc = Nmpc { n: 4, ..Nmpc::default() };
            let s = HovorkaState::basal(&p, g);
            let w = trajectory(g, target, nmpc.n, nmpc.step_min);
            let (_, seed_cost) = nmpc.seed(&p, s, &w, u_prev);
            let u = nmpc.sequence(&p, s, target, u_prev);
            prop_assert!(u.iter().all(|&r| (0.0..=nmpc.u_max).contains(&r)));
            prop_assert!(nmpc.cost(&p, s, &u, &w, u_prev) <= seed_cost);
        }
    }
}
