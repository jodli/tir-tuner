//! Integration and the resting state.
//!
//! Clamped forward Euler keeps every compartment non-negative; the common
//! crate's proofs cover the primitive.

use crate::derivative::{derivatives, egp, f01c, renal_excretion};
use crate::state::BodyState;
use crate::subject::VirtualSubject;
use tir_tuner_common::euler::{bisect_falling, clamped_forward_euler};

/// Euler step (min). W10 doesn't give its solver; at 0.25 min the fastest
/// rate times the step stays below 0.05. src: own-body-step
pub const DT_MIN: f64 = 0.25;

/// Advance one `dt` step under insulin influx `u_mu_per_min`.
pub fn step(subject: &VirtualSubject, state: &BodyState, u_mu_per_min: f64, dt: f64) -> BodyState {
    let d = derivatives(subject, state, u_mu_per_min);
    let e = |x, dx| clamped_forward_euler(x, dx, dt);
    BodyState {
        s1: e(state.s1, d.s1),
        s2: e(state.s2, d.s2),
        i: e(state.i, d.i),
        x1: e(state.x1, d.x1),
        x2: e(state.x2, d.x2),
        x3: e(state.x3, d.x3),
        q1: e(state.q1, d.q1),
        q2: e(state.q2, d.q2),
        g1: e(state.g1, d.g1),
        g2: e(state.g2, d.g2),
        c: e(state.c, d.c),
    }
}

/// Plasma insulin (mU/L) that holds glucose at rest at `g_mmol_per_l`, as
/// W10 p7 sets each subject to steady state at the start. Zero when uptake
/// beats EGP even without insulin.
///
/// At rest `x = S I` and `Q2 = x1 Q1 / (x2 + k12)`, so the `Q1` balance is
/// `EGP - F01c - FR = x1 Q1 x2 / (x2 + k12)`. The left side falls and the
/// right side rises with `I`, and at `I = 1/SIE` EGP is zero.
pub fn resting_insulin(subject: &VirtualSubject, g_mmol_per_l: f64) -> f64 {
    let q1 = g_mmol_per_l * subject.vg_l_per_kg;
    let uptake = f01c(subject, g_mmol_per_l) + renal_excretion(subject, g_mmol_per_l);
    let surplus = |i: f64| {
        let x1 = subject.sit_per_mu_l * i;
        let x2 = subject.sid_per_mu_l * i;
        egp(subject, subject.sie_per_mu_l * i) - uptake - x1 * q1 * x2 / (x2 + subject.k12_per_min)
    };
    if surplus(0.0) <= 0.0 {
        return 0.0;
    }
    bisect_falling(surplus, 0.0, 1.0 / subject.sie_per_mu_l)
}

/// The resting state at `g_mmol_per_l`: insulin from [`resting_insulin`],
/// the depots and actions at their steady state, gut empty, `C = G`.
pub fn rest_state(subject: &VirtualSubject, g_mmol_per_l: f64) -> BodyState {
    let i = resting_insulin(subject, g_mmol_per_l);
    let s = subject.insulin_flux(i) / subject.ka_per_min;
    let x1 = subject.sit_per_mu_l * i;
    let x2 = subject.sid_per_mu_l * i;
    let q1 = g_mmol_per_l * subject.vg_l_per_kg;
    BodyState {
        s1: s,
        s2: s,
        i,
        x1,
        x2,
        x3: subject.sie_per_mu_l * i,
        q1,
        q2: x1 * q1 / (x2 + subject.k12_per_min),
        g1: 0.0,
        g2: 0.0,
        c: g_mmol_per_l,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subject::W10_WEIGHT_KG;
    use tir_tuner_common::units::MIN_PER_HOUR;

    fn subject() -> VirtualSubject {
        VirtualSubject::nominal(W10_WEIGHT_KG)
    }

    #[test]
    fn rest_state_is_a_fixed_point() {
        let s = subject();
        for g in [4.5, 5.8, 7.8, 11.0] {
            let state = rest_state(&s, g);
            let d = derivatives(&s, &state, s.insulin_flux(state.i));
            for v in [
                d.s1, d.s2, d.i, d.x1, d.x2, d.x3, d.q1, d.q2, d.g1, d.g2, d.c,
            ] {
                assert!(v.abs() < 1e-12, "g {g}: derivative {v}");
            }
            assert!(state.i > 0.0);
            assert_eq!(state.plasma_glucose(&s), g);
        }
    }

    #[test]
    fn resting_insulin_rises_as_glucose_falls() {
        let s = subject();
        assert!(resting_insulin(&s, 4.5) > resting_insulin(&s, 5.8));
        assert!(resting_insulin(&s, 5.8) > resting_insulin(&s, 7.8));
        // Below 1/SIE, where EGP is still positive.
        assert!(resting_insulin(&s, 4.5) < 1.0 / 0.019);
    }

    #[test]
    fn no_basal_when_uptake_beats_egp() {
        // At 15 mmol/L F01c plus renal loss exceed EGP0.
        let s = subject();
        assert!(f01c(&s, 15.0) + renal_excretion(&s, 15.0) > 16.9e-3);
        assert_eq!(resting_insulin(&s, 15.0), 0.0);
    }

    #[test]
    fn steps_stay_at_rest() {
        let s = subject();
        let mut state = rest_state(&s, 5.8);
        let u = s.insulin_flux(state.i);
        for _ in 0..(24.0 * MIN_PER_HOUR / DT_MIN) as usize {
            state = step(&s, &state, u, DT_MIN);
        }
        assert!((state.plasma_glucose(&s) - 5.8).abs() < 1e-9);
    }

    #[test]
    fn meal_lifts_glucose_and_is_fully_absorbed() {
        let s = subject();
        let mut state = rest_state(&s, 5.8);
        let u = s.insulin_flux(state.i);
        state.eat(&s, 87.0, 1.0);
        let gut0 = state.g1;
        let (mut peak, mut absorbed) = (0.0_f64, 0.0);
        for _ in 0..(8.0 * MIN_PER_HOUR / DT_MIN) as usize {
            let (_, ug) = crate::derivative::gut(&s, state.g2);
            absorbed += ug * s.weight_kg * DT_MIN;
            state = step(&s, &state, u, DT_MIN);
            peak = peak.max(state.plasma_glucose(&s));
        }
        assert!(peak > 5.8 + 2.0, "peak {peak}");
        // The ceiling only delays: nearly all of W10's 87 g arrives.
        assert!(absorbed / gut0 > 0.99, "absorbed {}", absorbed / gut0);
    }

    proptest::proptest! {
        #[test]
        fn steps_stay_non_negative(dt in 0.05f64..0.5, u in 0.0f64..500.0, carbs in 0.0f64..150.0) {
            let s = subject();
            let mut state = rest_state(&s, 6.7);
            state.eat(&s, carbs, 1.0);
            for _ in 0..200 {
                state = step(&s, &state, u, dt);
            }
            for v in [state.s1, state.s2, state.i, state.x1, state.x2, state.x3,
                      state.q1, state.q2, state.g1, state.g2, state.c] {
                assert!(v.is_finite() && v >= 0.0, "{v}");
            }
        }
    }
}
