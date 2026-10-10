use chrono::NaiveTime;
use tir_tuner_common::settings::{AutoSettings, Schedule, default_auto_target};
use tir_tuner_common::units::MgDl;

use crate::hovorka::HovorkaParams;

/// Auto mode settings in the controller's units.
#[derive(Clone, Debug, PartialEq)]
pub struct AutoMode {
    target_mg_dl: Schedule,
    weight_kg: f64,
    tdd_u: f64,
}

impl AutoMode {
    /// From the user's settings.
    pub fn new(settings: &AutoSettings) -> Self {
        Self {
            target_mg_dl: settings.target().clone(),
            weight_kg: settings.weight_kg(),
            tdd_u: settings.total_daily_dose_u(),
        }
    }

    /// Personal target switched off, as for a user without settings.
    pub fn without_personal_target(weight_kg: f64, tdd_u: f64) -> Self {
        Self {
            target_mg_dl: default_auto_target(),
            weight_kg,
            tdd_u,
        }
    }

    /// Personal glucose target at `t`, mmol/L.
    pub fn target_mmol_l(&self, t: NaiveTime) -> f64 {
        MgDl(self.target_mg_dl.at(t)).to_mmol_l().0
    }

    /// Body weight entered in the app (kg).
    pub fn weight_kg(&self) -> f64 {
        self.weight_kg
    }

    /// Average total daily dose entered in the app (U).
    pub fn tdd_u(&self) -> f64 {
        self.tdd_u
    }

    /// The controller's model, from weight and TDD.
    pub fn model(&self) -> HovorkaParams {
        HovorkaParams::from_weight_and_tdd(self.weight_kg, self.tdd_u)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tir_tuner_common::settings::{Block, Profile, ProfileInput};

    #[test]
    fn controller_gets_the_users_target_and_weight() {
        let p = Profile::new(ProfileInput {
            carb_ratio: vec![Block::all_day(12.0)],
            correction_factor: vec![Block::all_day(100.0)],
            bolus_target: vec![Block::all_day(110.0)],
            auto_target: Some(vec![Block::all_day(120.0)]),
            insulin_action_hours: 2.5,
            max_bolus_u: 30.0,
            min_glucose_mg_dl: 70.0,
            weight_kg: 50.0,
            total_daily_dose_u: 36.0,
        })
        .unwrap();
        let auto = AutoMode::new(p.auto());
        assert_eq!((auto.weight_kg(), auto.tdd_u()), (50.0, 36.0));
        let m = auto.model();
        assert_eq!(m.weight_kg, 50.0);
        // Walsh eq 1: 36 U/day x 0.48 / 24 h.
        assert!((m.bir - 0.72).abs() < 1e-12);
        let noon = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        assert!((auto.target_mmol_l(noon) - 120.0 / 18.0156).abs() < 1e-9);
    }
}
