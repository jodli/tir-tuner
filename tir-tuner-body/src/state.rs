//! The eleven-compartment state of Wilinska 2010's model.

use crate::subject::VirtualSubject;
use tir_tuner_common::units::MMOL_PER_GRAM_CHO;

/// Glucoregulatory state of a virtual subject.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyState {
    /// Subcutaneous insulin, depot 1 (mU).
    pub s1: f64,
    /// Subcutaneous insulin, depot 2 (mU).
    pub s2: f64,
    /// Plasma insulin (mU/L).
    pub i: f64,
    /// Insulin action on glucose transport (/min).
    pub x1: f64,
    /// Insulin action on glucose disposal (/min).
    pub x2: f64,
    /// Insulin action on EGP (dimensionless).
    pub x3: f64,
    /// Accessible glucose mass (mmol/kg).
    pub q1: f64,
    /// Non-accessible glucose mass (mmol/kg).
    pub q2: f64,
    /// Gut glucose, compartment 1 (mmol).
    pub g1: f64,
    /// Gut glucose, compartment 2 (mmol).
    pub g2: f64,
    /// Interstitial glucose (mmol/L).
    pub c: f64,
}

impl BodyState {
    /// Every compartment empty.
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

    /// Plasma glucose `G = Q1 / VG` (mmol/L).
    pub fn plasma_glucose(&self, subject: &VirtualSubject) -> f64 {
        self.q1 / subject.vg_l_per_kg
    }

    /// A meal of `carbs_g`: W10's `Bio D(t)` into the first gut compartment,
    /// as an impulse. `bio_factor` is this meal's interoccasion variation.
    pub fn eat(&mut self, subject: &VirtualSubject, carbs_g: f64, bio_factor: f64) {
        self.g1 += subject.bio_fraction * bio_factor * carbs_g * MMOL_PER_GRAM_CHO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subject::W10_WEIGHT_KG;

    #[test]
    fn meal_lands_in_gut_at_bioavailability() {
        let s = VirtualSubject::nominal(W10_WEIGHT_KG);
        let mut state = BodyState::zero();
        state.eat(&s, 60.0, 1.1);
        assert!((state.g1 - 0.95 * 1.1 * 60.0 * 1000.0 / 180.156).abs() < 1e-9);
        assert_eq!(state.g2, 0.0);
        let before = state.g1;
        state.eat(&s, 10.0, 1.0);
        assert!((state.g1 - before - 0.95 * 10.0 * 1000.0 / 180.156).abs() < 1e-9);
    }
}
