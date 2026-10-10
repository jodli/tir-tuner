//! Right-hand sides of Wilinska 2010's model (Appendix, p12-13), per minute.
//!
//! The only continuous input is the subcutaneous insulin influx (mU/min).
//! Meals enter as an impulse into the gut, see [`BodyState::eat`].

use crate::state::BodyState;
use crate::subject::VirtualSubject;

/// Time derivatives of the eleven compartments (per minute).
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)]
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

/// Endogenous glucose production (mmol/kg/min), `EGP0 (1 - x3)` floored at
/// zero. W10 prints `1 + x3`, a sign typo against H02 eq 7 and H04 eq 1.
/// src: h04-egp
pub fn egp(subject: &VirtualSubject, x3: f64) -> f64 {
    (subject.egp0_mmol_per_kg_min * (1.0 - x3)).max(0.0)
}

/// Share of `F01` taken up at the glucose where `G/(G+1)` equals it.
/// src: w10-f01c
pub(crate) const F01_SATURATION: f64 = 0.85;

/// Non-insulin-dependent glucose uptake (mmol/kg/min), `F01/0.85 G/(G+1)`.
pub fn f01c(subject: &VirtualSubject, plasma_glucose_mmol_per_l: f64) -> f64 {
    subject.f01_mmol_per_kg_min / F01_SATURATION * plasma_glucose_mmol_per_l
        / (plasma_glucose_mmol_per_l + 1.0)
}

/// Renal glucose excretion (mmol/kg/min), zero below the threshold. W10 p12.
pub fn renal_excretion(subject: &VirtualSubject, plasma_glucose_mmol_per_l: f64) -> f64 {
    (subject.r_cl_per_min
        * (plasma_glucose_mmol_per_l - subject.r_thr_mmol_per_l)
        * subject.vg_l_per_kg)
        .max(0.0)
}

/// Gut absorption at gut mass `g2_mmol`: time-to-maximum (min) and the
/// glucose appearance (mmol/kg/min). Above the flux ceiling W10 (p12-13)
/// stretches `t_max` to `G2 / UG_ceil` for the whole chain, so absorption
/// slows and no carbs are lost.
pub fn gut(subject: &VirtualSubject, g2_mmol: f64) -> (f64, f64) {
    let w = subject.weight_kg;
    let t_max = subject
        .t_max_g_min
        .max(g2_mmol / (subject.ug_ceil_mmol_per_kg_min * w));
    let ug = (g2_mmol / (subject.t_max_g_min * w)).min(subject.ug_ceil_mmol_per_kg_min);
    (t_max, ug)
}

/// The full right-hand side under insulin influx `u_mu_per_min`.
pub fn derivatives(
    subject: &VirtualSubject,
    state: &BodyState,
    u_mu_per_min: f64,
) -> BodyDerivatives {
    let g = state.plasma_glucose(subject);
    let ka = subject.ka_per_min;
    let (t_max, ug) = gut(subject, state.g2);
    let q1 = egp(subject, state.x3) + ug - f01c(subject, g) - state.x1 * state.q1
        + subject.k12_per_min * state.q2
        - renal_excretion(subject, g);
    BodyDerivatives {
        s1: u_mu_per_min - ka * state.s1,
        s2: ka * (state.s1 - state.s2),
        i: ka * state.s2 / (subject.vi_l_per_kg * subject.weight_kg) - subject.ke_per_min * state.i,
        x1: subject.kb1_per_min * (subject.sit_per_mu_l * state.i - state.x1),
        x2: subject.kb2_per_min * (subject.sid_per_mu_l * state.i - state.x2),
        x3: subject.kb3_per_min * (subject.sie_per_mu_l * state.i - state.x3),
        q1,
        q2: state.x1 * state.q1 - (state.x2 + subject.k12_per_min) * state.q2,
        g1: -state.g1 / t_max,
        g2: (state.g1 - state.g2) / t_max,
        c: subject.ka_int_per_min * (g - state.c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subject::W10_WEIGHT_KG;

    fn subject() -> VirtualSubject {
        VirtualSubject::nominal(W10_WEIGHT_KG)
    }

    #[test]
    fn h04_egp_falls_linearly_to_zero() {
        let s = subject();
        assert_eq!(egp(&s, 0.0), 16.9e-3);
        assert!((egp(&s, 0.25) - 16.9e-3 * 0.75).abs() < 1e-15);
        assert_eq!(egp(&s, 1.0), 0.0);
        assert_eq!(egp(&s, 1.5), 0.0);
    }

    #[test]
    fn w10_f01c_is_f01_at_g_5_67() {
        let s = subject();
        let g = 0.85 / 0.15;
        assert!((f01c(&s, g) - 11.1e-3).abs() < 1e-15);
        assert!((f01c(&s, 1.0) - 11.1e-3 / 0.85 / 2.0).abs() < 1e-15);
    }

    #[test]
    fn w10_renal_excretion_above_threshold() {
        let s = subject();
        assert_eq!(renal_excretion(&s, 8.0), 0.0);
        assert!((renal_excretion(&s, 11.0) - 0.01 * 2.0 * 0.15).abs() < 1e-15);
    }

    #[test]
    fn w10_gut_ceiling_stretches_t_max() {
        let s = subject();
        // Below the ceiling: UG = G2 / (t_max w).
        let (t, ug) = gut(&s, 10.0);
        assert_eq!(t, 40.0);
        assert!((ug - 10.0 / (40.0 * W10_WEIGHT_KG)).abs() < 1e-15);
        // Above it: the flux sits at the ceiling and t_max = G2 / UG_ceil.
        let g2 = 500.0;
        let (t, ug) = gut(&s, g2);
        assert!((ug - 0.0275).abs() < 1e-15);
        assert!((t - g2 / (0.0275 * W10_WEIGHT_KG)).abs() < 1e-9);
    }

    /// W10 p12: one rate `ka` for both depots, `ka S2 / VI` into plasma.
    #[test]
    fn w10_insulin_chain() {
        let s = subject();
        let state = BodyState {
            s1: 30.0,
            s2: 20.0,
            i: 10.0,
            ..BodyState::zero()
        };
        let d = derivatives(&s, &state, 5.0);
        assert!((d.s1 - (5.0 - 0.0207 * 30.0)).abs() < 1e-12);
        assert!((d.s2 - 0.0207 * 10.0).abs() < 1e-12);
        assert!((d.i - (0.0207 * 20.0 / (0.12 * W10_WEIGHT_KG) - 0.14 * 10.0)).abs() < 1e-12);
        assert!((d.x3 - 0.024 * 0.019 * 10.0).abs() < 1e-15);
    }

    /// W10 p12: Q2 exchange and the gut chain.
    #[test]
    fn w10_glucose_and_gut_flows() {
        let s = subject();
        let state = BodyState {
            x1: 0.01,
            x2: 0.002,
            q1: 0.9,
            q2: 0.5,
            g1: 20.0,
            g2: 8.0,
            c: 5.0,
            ..BodyState::zero()
        };
        let d = derivatives(&s, &state, 0.0);
        assert!((d.q2 - (0.01 * 0.9 - (0.002 + 0.06) * 0.5)).abs() < 1e-15);
        assert!((d.g1 + 20.0 / 40.0).abs() < 1e-15);
        assert!((d.g2 - 12.0 / 40.0).abs() < 1e-15);
        assert!((d.c - 0.0933 * (6.0 - 5.0)).abs() < 1e-12);
        let ug = 8.0 / (40.0 * W10_WEIGHT_KG);
        let want = 16.9e-3 + ug - f01c(&s, 6.0) - 0.01 * 0.9 + 0.06 * 0.5;
        assert!((d.q1 - want).abs() < 1e-15);
    }
}
