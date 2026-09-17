//! Continuous-time right-hand sides of the physiological model.
//!
//! All rates are expressed per minute. The inputs are the subcutaneous
//! insulin influx `u_basal_mu_per_min` (mU/min, basal plus any bolus
//! spread over the time step) and the meal carbohydrate intake
//! `meal_g_per_min`. The gut stores are in mmol of glucose, so the meal
//! grams are converted with `MMOL_PER_GRAM_CHO`.

use crate::state::BodyState;
use crate::subject::VirtualSubject;
use tir_tuner_common::units::MMOL_PER_GRAM_CHO;

/// Inputs to the body over one time step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyInputs {
    /// Subcutaneous insulin injection rate (mU/min).
    pub u_basal_mu_per_min: f64,
    /// Meal carbohydrate intake (g/min).
    pub meal_g_per_min: f64,
}

/// Time derivatives of all eleven compartments (per minute).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyDerivatives {
    pub s1: f64,
    pub s2: f64,
    pub i: f64,
    pub x1: f64,
    pub x2: f64,
    pub x3: f64,
    pub q1: f64,
    pub q2: f64,
    pub g1: f64,
    pub g2: f64,
    pub c: f64,
}

/// Endogenous glucose production (mmol/kg/min), the aps-style
/// suppression model: `egp0 * 2^((x3_basal - x3)/S)`, capped at 3x basal
/// EGP, with `S = 0.5` (mU-invariant scale) and `x3_basal` the resting
/// value of the EGP action `sie * i_basal`. At rest the exponent is
/// zero, so EGP equals the published basal `egp0` exactly; rising
/// insulin action suppresses production. See module docs and
/// [`crate::subject`] for the divergence note from the published
/// `EGP0[1+x3]`.
pub fn egp(subject: &VirtualSubject, x3: f64) -> f64 {
    let x3_basal = subject.basale_x3();
    let uncapped = subject.egp0_mmol_per_kg_min
        * ((x3_basal - x3) / 0.5 * std::f64::consts::LN_2).exp();
    uncapped.min(3.0 * subject.egp0_mmol_per_kg_min)
}

/// Non-insulin-dependent glucose uptake (mmol/kg/min). The Michaelis-
/// Menten form `F01s * G / (G + 1)` with `F01s = F01/0.85`, from
/// Wilinska Table 1; `F01c = F01` exactly at `G = 5/0.85 = 5.88`? No:
/// `G/(G+1) = 0.85` gives `G = 5.67` mmol/L, so basal uptake equals the
/// published `F01` at a near-basal glucose of 5.67 mmol/L.
pub fn f01c(subject: &VirtualSubject, plasma_glucose_mmol_per_l: f64) -> f64 {
    let f01s = subject.f01_mmol_per_kg_min / 0.85;
    f01s * plasma_glucose_mmol_per_l / (plasma_glucose_mmol_per_l + 1.0)
}

/// Renal glucose excretion (mmol/kg/min), zero below the renal threshold.
pub fn renal_excretion(subject: &VirtualSubject, plasma_glucose_mmol_per_l: f64) -> f64 {
    if plasma_glucose_mmol_per_l > subject.r_thr_mmol_per_l {
        subject.r_cl_per_min
            * (plasma_glucose_mmol_per_l - subject.r_thr_mmol_per_l)
            * subject.vg_l_per_kg
    } else {
        0.0
    }
}

/// Gut glucose appearance rate (mmol/kg/min), clamped at `ug_ceil`.
pub fn gut_appearance(subject: &VirtualSubject, g2_mmol: f64) -> f64 {
    let rate = g2_mmol / (subject.t_max_g_min * subject.weight_kg);
    rate.min(subject.ug_ceil_mmol_per_kg_min)
}

/// Interstitial glucose equilibration rate (mmol/L per min).
pub fn interstitial_rate(subject: &VirtualSubject, plasma: f64, interstitial: f64) -> f64 {
    subject.ka_int_per_min * (plasma - interstitial)
}

/// Full right-hand side of the ODE system.
pub fn derivatives(
    subject: &VirtualSubject,
    state: &BodyState,
    inputs: &BodyInputs,
) -> BodyDerivatives {
    let g = state.plasma_glucose(subject);

    // Subcutaneous insulin absorption (Depot 1 -> Depot 2 -> plasma).
    let s1 = inputs.u_basal_mu_per_min - subject.ka_per_min * state.s1;
    let s2 = subject.ka_per_min * (state.s1 - state.s2);
    let i = subject.ka_per_min * state.s2 / (subject.vi_l_per_kg * subject.weight_kg)
        - subject.ke_per_min * state.i;

    // Remote insulin actions relax to the plasma insulin concentration.
    let x1 = subject.kb1_per_min * (subject.sit_per_mu_l * state.i - state.x1);
    let x2 = subject.kb2_per_min * (subject.sid_per_mu_l * state.i - state.x2);
    let x3 = subject.kb3_per_min * (subject.sie_per_mu_l * state.i - state.x3);

    // Gut chain with the meal input gated by bioavailability.
    let gut_input_mmol_per_min = subject.bio_fraction * inputs.meal_g_per_min * MMOL_PER_GRAM_CHO;
    let g1 = gut_input_mmol_per_min - state.g1 / subject.t_max_g_min;
    let g2 = state.g1 / subject.t_max_g_min - state.g2 / subject.t_max_g_min;

    let ug = gut_appearance(subject, state.g2);

    // Glucose masses: accessible Q1, non-accessible Q2.
    let q1 = egp(subject, state.x3)
        + ug
        - f01c(subject, g)
        - state.x1 * state.q1
        + subject.k12_per_min * state.q2
        - renal_excretion(subject, g);
    let q2 = state.x1 * state.q1
        - state.x2 * state.q2
        - subject.k12_per_min * state.q2;

    let c = interstitial_rate(subject, g, state.c);

    BodyDerivatives {
        s1,
        s2,
        i,
        x1,
        x2,
        x3,
        q1,
        q2,
        g1,
        g2,
        c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mean_state() -> (VirtualSubject, BodyState) {
        (VirtualSubject::population_mean(), BodyState::zero())
    }

    #[test]
    fn f01c_is_michaelis_menten() {
        let (subject, _) = mean_state();
        // F01s * G/(G+1); equals F01 at G = 5.67 (where G/(G+1) = 0.85).
        let f01 = subject.f01_mmol_per_kg_min;
        let g_bal = 0.85 / (1.0 - 0.85);
        assert!((f01c(&subject, g_bal) - f01).abs() < 1e-12);
        // Saturates at F01s, never above.
        assert!(f01c(&subject, 1e9) <= f01 / 0.85 + 1e-12);
        assert_eq!(f01c(&subject, 0.0), 0.0);
    }

    #[test]
    fn renal_excretion_drops_out_below_threshold() {
        let (subject, _) = mean_state();
        assert_eq!(renal_excretion(&subject, 8.9), 0.0);
        assert!(renal_excretion(&subject, 10.0) > 0.0);
    }

    #[test]
    fn egp_suppresses_with_insulin_action() {
        let (subject, _) = mean_state();
        let low = egp(&subject, 0.0);
        let basal_x3 = subject.basale_x3();
        let high = egp(&subject, basal_x3 * 10.0);
        // Zero insulin action still raises EGP above basal (Hepatic
        // glucose output rises without insulin), but stays capped.
        assert!(low > subject.egp0_mmol_per_kg_min);
        assert!(low <= 3.0 * subject.egp0_mmol_per_kg_min);
        assert!(high < subject.egp0_mmol_per_kg_min);
        // Anchor: at the basal action the production equals basal EGP.
        assert!((egp(&subject, basal_x3) - subject.egp0_mmol_per_kg_min).abs() < 1e-9);
    }

    #[test]
    fn gut_appearance_is_capped() {
        let (subject, _) = mean_state();
        assert!(gut_appearance(&subject, 1e6) <= subject.ug_ceil_mmol_per_kg_min + 1e-12);
        assert_eq!(gut_appearance(&subject, 0.0), 0.0);
    }

    proptest::proptest! {
        #[test]
        fn derivatives_finite_for_bounded_states(
            s1 in 0.0f64..100.0,
            s2 in 0.0f64..100.0,
            i in 0.0f64..200.0,
            q1 in 0.1f64..20.0,
            q2 in 0.0f64..20.0,
            g1 in 0.0f64..150.0,
            g2 in 0.0f64..150.0,
            c in 0.0f64..20.0,
            meal in 0.0f64..5.0,
            u in 0.0f64..500.0,
        ) {
            let subject = VirtualSubject::population_mean();
            let state = BodyState {
                s1, s2, i,
                x1: subject.sit_per_mu_l * i,
                x2: subject.sid_per_mu_l * i,
                x3: subject.sie_per_mu_l * i,
                q1, q2, g1, g2, c,
            };
            let inputs = BodyInputs { u_basal_mu_per_min: u, meal_g_per_min: meal };
            let d = derivatives(&subject, &state, &inputs);
            for v in [d.s1, d.s2, d.i, d.x1, d.x2, d.x3, d.q1, d.q2, d.g1, d.g2, d.c] {
                assert!(v.is_finite());
            }
        }
    }
}