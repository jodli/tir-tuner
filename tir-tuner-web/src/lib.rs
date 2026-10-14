//! The wasm binding the patient-site demo calls: run one virtual day with a
//! possibly changed breakfast carb ratio, as JSON in and JSON out. Pure
//! plumbing over `tir-tuner-sim`; it holds no medical numbers beyond the
//! scenario's structural 06:00 position.

use serde::{Deserialize, Serialize};
use tir_tuner_common::metrics::range_bands_pct;
use tir_tuner_common::units::mg_dl_series;
use wasm_bindgen::prelude::*;

/// Start of the breakfast the slider retunes, minutes since scenario start.
const BREAKFAST_START_MIN: f64 = 360.0;

#[derive(Deserialize)]
pub struct DayRequest {
    pub seed: u64,
    #[serde(default)]
    pub breakfast_cr_g_per_u: Option<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct DayResult {
    /// Sample times, minutes since start, one per 15-min controller period.
    pub t_min: Vec<f64>,
    /// Sensor reading at each sample, mg/dL (the same signal the report
    /// panels plot).
    pub glucose_mg_dl: Vec<f64>,
    pub bolus_u_breakfast: f64,
    pub tir_pct: f64,
    pub tbr_pct: f64,
    pub tar_pct: f64,
    /// `[start_min, carbs_g]` per meal, the scenario's meal schedule.
    pub meals: Vec<[f64; 2]>,
}

/// Run `realistic_day(seed)`; with `breakfast_cr_g_per_u` set, the 06:00
/// bolus becomes carbs over that ratio instead of the configured one.
#[wasm_bindgen]
pub fn run_day(request: &str) -> Result<String, JsValue> {
    console_error_panic_hook::set_once();
    let req: DayRequest =
        serde_json::from_str(request).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let mut cfg = tir_tuner_sim::realistic_day(req.seed);
    let bolus = cfg
        .boluses
        .iter_mut()
        .find(|b| b.start_min == BREAKFAST_START_MIN)
        .ok_or_else(|| missing("bolus"))?;
    if let Some(cr) = req.breakfast_cr_g_per_u {
        let carbs = cfg
            .meals
            .iter()
            .find(|m| m.start_min == BREAKFAST_START_MIN)
            .ok_or_else(|| missing("meal"))?
            .carbs_g;
        bolus.units = carbs / cr;
    }
    let bolus_u_breakfast = bolus.units;
    let trace = tir_tuner_sim::simulate(&cfg);
    let readings = mg_dl_series(&trace.reading_mmol_per_l);
    let bands = range_bands_pct(&readings);
    let result = DayResult {
        t_min: trace.t_min,
        glucose_mg_dl: readings.iter().map(|g| g.0).collect(),
        bolus_u_breakfast,
        tbr_pct: bands[0] + bands[1],
        tir_pct: bands[2],
        tar_pct: bands[3] + bands[4],
        meals: cfg.meals.iter().map(|m| [m.start_min, m.carbs_g]).collect(),
    };
    serde_json::to_string(&result).map_err(|e| JsValue::from_str(&e.to_string()))
}

fn missing(what: &str) -> JsValue {
    JsValue::from_str(&format!("realistic_day: no {what} at 06:00"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tir_tuner_common::metrics::mean_glucose;
    use tir_tuner_common::units::MgDl;

    const SEED_JSON: &str = r#"{"seed":1}"#;
    const RETUNED_JSON: &str = r#"{"seed":1,"breakfast_cr_g_per_u":20.0}"#;

    fn decode(json: &str) -> DayResult {
        serde_json::from_str(json).unwrap()
    }

    fn mean(v: &[f64]) -> f64 {
        let mg: Vec<MgDl> = v.iter().map(|&g| MgDl(g)).collect();
        mean_glucose(&mg).0
    }

    #[test]
    fn base_run_matches_the_configured_day() {
        let day = decode(&run_day(SEED_JSON).unwrap());
        assert_eq!(day.t_min.len(), 96);
        assert_eq!(day.t_min[95], 1425.0);
        let base = tir_tuner_sim::realistic_day(1);
        assert_eq!(base.boluses[0].start_min, BREAKFAST_START_MIN);
        assert_eq!(day.bolus_u_breakfast, base.boluses[0].units);
    }

    #[test]
    fn weaker_carb_ratio_raises_glucose() {
        let base = decode(&run_day(SEED_JSON).unwrap());
        let retuned = decode(&run_day(RETUNED_JSON).unwrap());
        assert_eq!(retuned.bolus_u_breakfast, 3.0);
        assert!(mean(&retuned.glucose_mg_dl) > mean(&base.glucose_mg_dl));
    }
}
