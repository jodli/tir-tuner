//! The CamAPS FX bolus calculator as the user manual describes it (PDF pages 62-64).

use chrono::NaiveTime;
use tir_tuner_common::settings::BolusSettings;

/// A suggested bolus, split the way the calculator computes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Suggestion {
    /// carbs / carb ratio.
    pub meal_u: f64,
    /// (glucose - target) / correction factor, less insulin on board.
    /// Negative below target, which reduces the meal part.
    pub correction_u: f64,
    /// meal + correction, at least 0, at most the max bolus.
    pub total_u: f64,
}

/// The calculator doesn't suggest a bolus below the minimum glucose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlucoseTooLow;

pub fn suggest(
    s: &BolusSettings,
    at: NaiveTime,
    carbs_g: f64,
    glucose_mg_dl: f64,
    insulin_on_board_u: f64,
) -> Result<Suggestion, GlucoseTooLow> {
    if glucose_mg_dl < s.min_glucose_mg_dl() {
        return Err(GlucoseTooLow);
    }
    let meal_u = carbs_g / s.carb_ratio().at(at);
    let raw = (glucose_mg_dl - s.target().at(at)) / s.correction_factor().at(at);
    // Insulin on board only eats into a positive correction, never into the
    // meal part (p64).
    let correction_u = if raw > 0.0 {
        (raw - insulin_on_board_u).max(0.0)
    } else {
        raw
    };
    let total_u = (meal_u + correction_u).clamp(0.0, s.max_bolus_u());
    Ok(Suggestion {
        meal_u,
        correction_u,
        total_u,
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

    fn noon() -> NaiveTime {
        NaiveTime::from_hms_opt(12, 0, 0).unwrap()
    }

    #[test]
    fn insulin_on_board_never_reduces_the_meal_part() {
        let s = suggest(&settings(), noon(), 60.0, 210.0, 3.0).unwrap();
        assert_eq!((s.meal_u, s.correction_u, s.total_u), (6.0, 0.0, 6.0));
    }

    #[test]
    fn below_target_reduces_the_meal_part() {
        let s = suggest(&settings(), noon(), 15.0, 80.0, 0.0).unwrap();
        assert_eq!(s.total_u, 1.2);
        assert_eq!(
            suggest(&settings(), noon(), 10.0, 70.0, 0.0)
                .unwrap()
                .total_u,
            0.6
        );
        // Hypo treatment: carbs entered, nothing delivered.
        assert_eq!(
            suggest(&settings(), noon(), 3.0, 70.0, 0.0)
                .unwrap()
                .total_u,
            0.0
        );
    }

    #[test]
    fn capped_and_refused() {
        assert_eq!(
            suggest(&settings(), noon(), 120.0, 110.0, 0.0)
                .unwrap()
                .total_u,
            8.0
        );
        assert_eq!(
            suggest(&settings(), noon(), 40.0, 69.0, 0.0),
            Err(GlucoseTooLow)
        );
    }
}
