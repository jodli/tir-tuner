use chrono::NaiveTime;
use tir_tuner_common::settings::{AutoSettings, Schedule, default_auto_target};
use tir_tuner_common::units::MgDl;

/// Auto mode settings in the controller's units.
#[derive(Clone, Debug, PartialEq)]
pub struct AutoMode {
    target_mg_dl: Schedule,
    weight_kg: f64,
}

impl AutoMode {
    /// From the user's settings.
    pub fn new(settings: &AutoSettings) -> Self {
        Self {
            target_mg_dl: settings.target().clone(),
            weight_kg: settings.weight_kg(),
        }
    }

    /// Personal target switched off, as for a user without settings.
    pub fn without_personal_target(weight_kg: f64) -> Self {
        Self {
            target_mg_dl: default_auto_target(),
            weight_kg,
        }
    }

    /// Personal glucose target at `t`, mmol/L.
    pub fn target_mmol_l(&self, t: NaiveTime) -> f64 {
        MgDl(self.target_mg_dl.at(t)).to_mmol_l().0
    }

    /// Body weight the model scales glucose and insulin volumes by.
    pub fn weight_kg(&self) -> f64 {
        self.weight_kg
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
        assert_eq!(auto.weight_kg(), 50.0);
        let noon = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        assert!((auto.target_mmol_l(noon) - 120.0 / 18.0156).abs() < 1e-9);
    }
}
