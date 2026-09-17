//! Integration: clamped forward Euler over the physiological state.
//!
//! The step size is fixed at [`DT_MIN`]; RK4 is deliberately not used
//! because non-negativity is not an invariant under RK4 overshoot. Each
//! compartment is routed through `clamped_forward_euler` from common,
//! the same primitive the aps crate proofs cover, so non-negativity and
//! finiteness carry over here.

use crate::derivative::{derivatives, BodyInputs};
use crate::state::BodyState;
use crate::subject::VirtualSubject;
use tir_tuner_common::euler::clamped_forward_euler;
use tir_tuner_common::units::MU_PER_UNIT;

/// Fixed integration time step (minutes). Matches the aps controller
/// granularity.
pub const DT_MIN: f64 = 0.25;

/// Advance the body by one `dt` minute step with the clamped Euler rule.
pub fn step(
    subject: &VirtualSubject,
    state: &BodyState,
    inputs: &BodyInputs,
    dt: f64,
) -> BodyState {
    let d = derivatives(subject, state, inputs);
    BodyState {
        s1: clamped_forward_euler(state.s1, d.s1, dt),
        s2: clamped_forward_euler(state.s2, d.s2, dt),
        i: clamped_forward_euler(state.i, d.i, dt),
        x1: clamped_forward_euler(state.x1, d.x1, dt),
        x2: clamped_forward_euler(state.x2, d.x2, dt),
        x3: clamped_forward_euler(state.x3, d.x3, dt),
        q1: clamped_forward_euler(state.q1, d.q1, dt),
        q2: clamped_forward_euler(state.q2, d.q2, dt),
        g1: clamped_forward_euler(state.g1, d.g1, dt),
        g2: clamped_forward_euler(state.g2, d.g2, dt),
        c: clamped_forward_euler(state.c, d.c, dt),
    }
}

/// The basal steady state of the subject: plasma and glucose at rest
/// under the basal insulin requirement and no meal, reached by
/// integrating the real model (not a closed-form shortcut) from a zero
/// state until the largest compartment derivative drops below
/// [`BASAL_EPS`]. The slowest mode is `x1` (time constant ~290 min), so
/// the walk caps at a generous horizon in case a subject converges
/// slowly.
pub const BASAL_EPS: f64 = 1e-8;
const MAX_BASAL_STEPS: usize = 200_000; // ~34 virtual hours at dt 0.25

pub fn basal_steady_state(subject: &VirtualSubject) -> BodyState {
    let inputs = BodyInputs {
        u_basal_mu_per_min: subject.bir_u_per_h / 60.0 * MU_PER_UNIT,
        meal_g_per_min: 0.0,
    };
    let mut state = BodyState::zero();
    for _ in 0..MAX_BASAL_STEPS {
        let d = derivatives(subject, &state, &inputs);
        let largest = max_abs_derivative(&d);
        if largest < BASAL_EPS {
            return state;
        }
        state = step(subject, &state, &inputs, DT_MIN);
    }
    state
}

fn max_abs_derivative(d: &crate::derivative::BodyDerivatives) -> f64 {
    [
        d.s1, d.s2, d.i, d.x1, d.x2, d.x3, d.q1, d.q2, d.g1, d.g2, d.c,
    ]
    .iter()
    .fold(0.0_f64, |acc, &v| acc.max(v.abs()))
}

/// Construct a state at a given presenting plasma glucose (mg/dL) with
/// every non-glucose compartment at its basal rest value. One of the
/// three ingest scenario entry points (100 / 80 / 180 mg/dL).
///
/// The non-accessible glucose mass is set to its basal balance point
/// `q2 = x1*q1/(x2 + k12)` so the model does not start with a spur of
/// inter-compartment flux.
pub fn admit_state(subject: &VirtualSubject, glucose_mg_per_dl: f64) -> BodyState {
    use tir_tuner_common::units::mg_per_dl_to_mmol_per_l;
    let g = mg_per_dl_to_mmol_per_l(glucose_mg_per_dl);

    let u_basal_mu_per_min = subject.bir_u_per_h / 60.0 * MU_PER_UNIT;
    let s1 = u_basal_mu_per_min / subject.ka_per_min;
    let i = u_basal_mu_per_min / (subject.vi_l_per_kg * subject.weight_kg * subject.ke_per_min);
    let x1 = subject.sit_per_mu_l * i;
    let x2 = subject.sid_per_mu_l * i;
    let x3 = subject.sie_per_mu_l * i;
    let q1 = g * subject.vg_l_per_kg;
    let q2 = x1 * q1 / (x2 + subject.k12_per_min);

    BodyState {
        s1,
        s2: s1,
        i,
        x1,
        x2,
        x3,
        q1,
        q2,
        g1: 0.0,
        g2: 0.0,
        c: g,
    }
}

/// A copy of `subject` whose basal insulin requirement has been titrated
/// so the resting glucose (no meal, no bolus) equals `target_mmol_per_l`.
///
/// The Cambridge simulator calibrates each virtual subject this way;
/// the published F01/EGP0/insulin-sensitivity balance by itself does not
/// pin the resting glucose to a euglycemic value. Bisection over the
/// basal rate, three hours of simulated time per evaluation.
pub fn with_basal_glucose(subject: &VirtualSubject, target_mmol_per_l: f64) -> VirtualSubject {
    fn resting_glucose(subject: &VirtualSubject) -> f64 {
        basal_steady_state(subject).plasma_glucose(subject)
    }

    // Monotone: more basal insulin pulls resting glucose down. `lo` is
    // the low-insulin (high-glucose) bracket, `hi` the high-insulin one.
    let mut lo = subject.clone();
    while resting_glucose(&lo) < target_mmol_per_l {
        lo.bir_u_per_h *= 0.5;
    }
    let mut hi = subject.clone();
    while resting_glucose(&hi) > target_mmol_per_l {
        hi.bir_u_per_h *= 1.5;
    }
    for _ in 0..40 {
        let mid = VirtualSubject {
            bir_u_per_h: 0.5 * (lo.bir_u_per_h + hi.bir_u_per_h),
            ..*subject
        };
        if resting_glucose(&mid) > target_mmol_per_l {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    VirtualSubject {
        bir_u_per_h: 0.5 * (lo.bir_u_per_h + hi.bir_u_per_h),
        ..*subject
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derivative::derivatives;
    use crate::subject::cohort;

    #[test]
    fn basal_state_is_a_fixed_point() {
        for subject in [VirtualSubject::population_mean(), cohort(1, 3)[0]] {
            let state = basal_steady_state(&subject);
            let inputs = BodyInputs {
                u_basal_mu_per_min: subject.bir_u_per_h / 60.0 * MU_PER_UNIT,
                meal_g_per_min: 0.0,
            };
            let d = derivatives(&subject, &state, &inputs);
            for v in [d.s1, d.s2, d.i, d.x1, d.x2, d.x3, d.q1, d.q2, d.g1, d.g2, d.c] {
                assert!(v.abs() < 1e-6, "compartment derivative {} not at rest", v);
            }
            // The resting glucose is whatever balances production and
            // utilization; it must be finite and above zero.
            let g = state.plasma_glucose(&subject);
            assert!(g.is_finite() && g > 0.0);
        }
    }

    #[test]
    fn calibrated_subject_rests_at_target() {
        let subject = VirtualSubject::population_mean();
        let target = 5.8;
        let calibrated = with_basal_glucose(&subject, target);
        let state = basal_steady_state(&calibrated);
        assert!((state.plasma_glucose(&calibrated) - target).abs() < 0.15);
    }

    #[test]
    fn admit_states_give_requested_start() {
        for (mg, expected_mmol) in [(100, 5.55), (80, 4.44), (180, 9.99)] {
            let subject = VirtualSubject::population_mean();
            let state = admit_state(&subject, mg as f64);
            let g = state.plasma_glucose(&subject);
            assert!((g - expected_mmol).abs() < 0.05, "start {} got {}", mg, g);
            // Interstitial equals plasma at admit.
            assert!((state.c - g).abs() < 1e-9);
        }
    }

    #[test]
    fn step_moves_toward_basal_and_stays_non_negative() {
        let subject = with_basal_glucose(&VirtualSubject::population_mean(), 5.8);
        let mut state = admit_state(&subject, 180.0);
        let inputs = BodyInputs {
            u_basal_mu_per_min: subject.bir_u_per_h / 60.0 * MU_PER_UNIT,
            meal_g_per_min: 0.0,
        };
        // Run six virtual hours; the high start should drift down.
        for _ in 0..6 * 60 * 4 {
            state = step(&subject, &state, &inputs, DT_MIN);
        }
        let g = state.plasma_glucose(&subject);
        assert!(g < 7.0, "glucose fell from 9.99 to {}", g);
        let all_non_neg =
            [state.s1, state.s2, state.i, state.x1, state.x2, state.x3, state.q1, state.q2, state.g1, state.g2, state.c]
                .iter()
                .all(|&v| v >= 0.0);
        assert!(all_non_neg);
    }

    #[test]
    fn meal_lifts_glucose() {
        let subject = with_basal_glucose(&VirtualSubject::population_mean(), 5.8);
        let mut state = basal_steady_state(&subject);
        let baseline = state.plasma_glucose(&subject);
        let mut peak = baseline;
        // 50 g meal over 15 minutes.
        let meal = BodyInputs {
            u_basal_mu_per_min: subject.bir_u_per_h / 60.0 * MU_PER_UNIT,
            meal_g_per_min: 50.0 / 15.0,
        };
        let after = BodyInputs {
            u_basal_mu_per_min: subject.bir_u_per_h / 60.0 * MU_PER_UNIT,
            meal_g_per_min: 0.0,
        };
        for k in 0..(8 * 60 * 4) {
            let inputs = if k < 15 * 4 { &meal } else { &after };
            state = step(&subject, &state, inputs, DT_MIN);
            peak = peak.max(state.plasma_glucose(&subject));
        }
        assert!(peak > baseline + 0.5, "meal peak {} over {}", peak, baseline);
    }

    proptest::proptest! {
        #[test]
        fn random_walk_stays_non_negative(dt in 0.05f64..0.5) {
            // No basal calibration here; the walk only checks
            // non-negativity and finiteness under clamped Euler.
            let subject = VirtualSubject::population_mean();
            let mut state = admit_state(&subject, 120.0);
            let inputs = BodyInputs {
                u_basal_mu_per_min: subject.bir_u_per_h / 60.0 * MU_PER_UNIT,
                meal_g_per_min: 2.0,
            };
            for _ in 0..200 {
                state = step(&subject, &state, &inputs, dt);
            }
            for v in [state.s1, state.s2, state.i, state.x1, state.x2, state.x3,
                      state.q1, state.q2, state.g1, state.g2, state.c] {
                assert!(v.is_finite());
                assert!(v >= 0.0, "compartment went negative: {}", v);
            }
        }
    }
}