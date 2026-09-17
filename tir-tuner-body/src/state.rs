//! The eleven-compartment physiological state.
//!
//! Compartments in order: subcutaneous insulin depots `s1`/`s2` (mU),
//! plasma insulin `i` (mU/L), remote actions `x1`/`x2`/`x3` (mU/L),
//! glucose masses `q1`/`q2` (mmol/kg), gut stores `g1`/`g2` (mmol),
//! interstitial glucose `c` (mmol/L).

use crate::subject::VirtualSubject;

/// Full glucoregulatory state of a virtual subject.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyState {
    /// Subcutaneous insulin mass, depot 1 (mU).
    pub s1: f64,
    /// Subcutaneous insulin mass, depot 2 (mU).
    pub s2: f64,
    /// Plasma insulin concentration (mU/L).
    pub i: f64,
    /// Remote insulin action on glucose transport (mU/L).
    pub x1: f64,
    /// Remote insulin action on glucose disposal (mU/L).
    pub x2: f64,
    /// Remote insulin action on EGP suppression (mU/L).
    pub x3: f64,
    /// Accessible glucose mass (mmol/kg).
    pub q1: f64,
    /// Non-accessible glucose mass (mmol/kg).
    pub q2: f64,
    /// Gut glucose store, absorption depot 1 (mmol).
    pub g1: f64,
    /// Gut glucose store, absorption depot 2 (mmol).
    pub g2: f64,
    /// Interstitial glucose concentration (mmol/L).
    pub c: f64,
}

impl BodyState {
    /// The basal steady state: no insulin bolus, no meal, plasma and
    /// glucose compartments at rest.
    ///
    /// Subcutaneous chain at rest: `s2 = s1 = u_basal/ka` with `u_basal`
    /// the basal mass rate (mU/min); plasma `i = u/(vi*w*ke)`; remote
    /// actions track `i`; gut empty; interstitial glucose `c` equals
    /// plasma glucose `g = q1/vg`. The glucose masses must balance the
    /// expression `EGP - F01 - FR`, solved by simulation in
    /// [`crate::solver::basal_steady_state`].
    pub fn zero() -> Self {
        Self {
            s1: 0.0,
            s2: 0.0,
            i: 0.0,
            x1: 0.0,
            x2: 0.0,
            x3: 0.0,
            q1: 0.0,
            q2: 0.0,
            g1: 0.0,
            g2: 0.0,
            c: 0.0,
        }
    }

    /// Accessible plasma glucose concentration (mmol/L).
    pub fn plasma_glucose(&self, subject: &VirtualSubject) -> f64 {
        self.q1 / subject.vg_l_per_kg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_state_gives_zero_plasma_glucose() {
        let subject = VirtualSubject::population_mean();
        assert_eq!(BodyState::zero().plasma_glucose(&subject), 0.0);
    }

    #[test]
    fn plasma_glucose_inverts_distribution_volume() {
        let subject = VirtualSubject::population_mean();
        let state = BodyState {
            q1: 0.75,
            ..BodyState::zero()
        };
        assert!((state.plasma_glucose(&subject) - 5.0).abs() < 1e-12);
    }
}