//! The CamAPS FX bolus calculator as the user manual describes it (PDF pages
//! 50-52, 62-64).

use chrono::NaiveTime;
use tir_tuner_common::settings::BolusSettings;
use tir_tuner_common::units::{Grams, MgDl, Units};

/// A suggested bolus, split the way the calculator computes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Suggestion {
    pub meal: Units,
    /// Negative below target, which reduces the meal part.
    pub correction: Units,
    /// Not rounded to the pump's 0.1 U step: the manual doesn't say which way
    /// the app rounds. src: open-rounding
    pub total: Units,
}

#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
pub enum Refused {
    /// src: man-min-glucose-refuse
    #[error("glucose below the minimum for a bolus suggestion")]
    GlucoseTooLow,
    /// src: open-iob-below-target
    #[error("insulin on board below target: the manual doesn't say what the app does")]
    IobBelowTarget,
}

/// `glucose: None` is a meal bolus in Auto mode, where the app greys out
/// glucose and doesn't use it. src: man-auto-meal-no-glucose
pub fn suggest(
    s: &BolusSettings,
    at: NaiveTime,
    carbs: Grams,
    glucose: Option<MgDl>,
    insulin_on_board: Units,
) -> Result<Suggestion, Refused> {
    // src: man-carb-ratio
    let meal = carbs / s.carb_ratio(at);
    let correction = match glucose {
        None => Units(0.0),
        Some(g) if g < s.min_glucose() => return Err(Refused::GlucoseTooLow),
        Some(g) => {
            // src: man-correction-factor
            let raw = (g - s.target(at)) / s.correction_factor(at);
            if raw.0 >= 0.0 {
                // Insulin on board only comes off the correction, never off
                // the meal part. At target the correction is 0 either way.
                // src: man-iob-correction-only
                (raw - insulin_on_board).max(Units(0.0))
            } else if insulin_on_board.0 > 0.0 {
                return Err(Refused::IobBelowTarget);
            } else {
                // src: man-negative-correction
                raw
            }
        }
    };
    // No negative bolus; capped at the max bolus. src: man-max-bolus-cap
    let total = (meal + correction).clamp(Units(0.0), s.max_bolus());
    Ok(Suggestion {
        meal,
        correction,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tir_tuner_common::settings::{Block, Profile, ProfileInput};

    fn settings() -> BolusSettings {
        let all_day = |v| vec![Block::all_day(v)];
        let input = ProfileInput {
            carb_ratio: all_day(10.0),
            correction_factor: all_day(100.0),
            bolus_target: all_day(110.0),
            auto_target: None,
            insulin_action_hours: 2.5,
            max_bolus_u: 8.0,
            min_glucose_mg_dl: 70.0,
            weight_kg: 50.0,
            total_daily_dose_u: 36.0,
        };
        Profile::new(input).unwrap().bolus().clone()
    }

    fn total(carbs: f64, glucose: Option<f64>, iob: f64) -> Result<f64, Refused> {
        let noon = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        suggest(
            &settings(),
            noon,
            Grams(carbs),
            glucose.map(MgDl),
            Units(iob),
        )
        .map(|s| s.total.0)
    }

    #[test]
    fn man_carb_ratio_and_correction_factor() {
        // 60 g / 10 g/U + (210 - 110) / 100 mg/dL/U.
        assert_eq!(total(60.0, Some(210.0), 0.0), Ok(7.0));
    }

    #[test]
    fn man_iob_correction_only() {
        assert_eq!(total(60.0, Some(210.0), 0.4), Ok(6.6));
        assert_eq!(total(60.0, Some(210.0), 3.0), Ok(6.0));
        assert_eq!(total(60.0, Some(110.0), 3.0), Ok(6.0));
    }

    #[test]
    fn man_negative_correction_reduces_the_meal_part() {
        assert_eq!(total(15.0, Some(80.0), 0.0), Ok(1.2));
        assert_eq!(total(3.0, Some(70.0), 0.0), Ok(0.0));
    }

    #[test]
    fn man_auto_meal_no_glucose() {
        assert_eq!(total(60.0, None, 3.0), Ok(6.0));
    }

    #[test]
    fn man_max_bolus_cap() {
        assert_eq!(total(120.0, Some(110.0), 0.0), Ok(8.0));
    }

    #[test]
    fn man_min_glucose_refuse() {
        assert_eq!(total(40.0, Some(69.0), 0.0), Err(Refused::GlucoseTooLow));
        assert_eq!(total(40.0, Some(70.0), 0.0), Ok(3.6));
    }

    #[test]
    fn open_iob_below_target() {
        assert_eq!(total(40.0, Some(100.0), 0.5), Err(Refused::IobBelowTarget));
    }
}
