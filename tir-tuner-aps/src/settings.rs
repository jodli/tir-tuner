use chrono::NaiveTime;
use tir_tuner_common::settings::{AutoSettings, Schedule};
use tir_tuner_common::units::mg_per_dl_to_mmol_per_l;

use crate::TARGET_GLUCOSE_MMOL_L;
use crate::hovorka::HovorkaParams;

/// Auto mode settings in the controller's units.
#[derive(Clone, Debug, PartialEq)]
pub struct AutoMode {
    /// `None`: no personal target, [`TARGET_GLUCOSE_MMOL_L`] all day.
    target_mg_dl: Option<Schedule>,
    weight_kg: f64,
}

impl AutoMode {
    /// From the user's settings.
    pub fn new(settings: &AutoSettings) -> Self {
        Self {
            target_mg_dl: Some(settings.target().clone()),
            weight_kg: settings.weight_kg(),
        }
    }

    /// Personal glucose target at `t`, mmol/L.
    pub fn target_mmol_l(&self, t: NaiveTime) -> f64 {
        self.target_mg_dl
            .as_ref()
            .map_or(TARGET_GLUCOSE_MMOL_L, |s| mg_per_dl_to_mmol_per_l(s.at(t)))
    }

    /// Body weight the model scales glucose and insulin volumes by.
    pub fn weight_kg(&self) -> f64 {
        self.weight_kg
    }
}

/// No personal target, the model's reference weight.
impl Default for AutoMode {
    fn default() -> Self {
        Self {
            target_mg_dl: None,
            weight_kg: HovorkaParams::default().weight_kg,
        }
    }
}
