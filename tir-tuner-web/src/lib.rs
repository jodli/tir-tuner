//! The wasm bindings the patient site calls: run one virtual day (the demo
//! chapter) or a reader-composed playground scenario, plus the bolus
//! calculator, as JSON in and JSON out. Pure plumbing over the model crates;
//! it holds no medical numbers beyond the demo's structural 06:00 position.

use chrono::{NaiveTime, TimeDelta};
use serde::{Deserialize, Serialize};
use tir_tuner_body::subject::{VirtualSubject, W10_WEIGHT_KG};
use tir_tuner_bolus::suggest;
use tir_tuner_common::metrics::range_bands_pct;
use tir_tuner_common::settings::{Block, Profile, ProfileInput};
use tir_tuner_common::units::{Grams, MgDl, Units, mg_dl_series};
use tir_tuner_sim::{Bolus, Meal, SimConfig, simulate};
use wasm_bindgen::prelude::*;

/// Start of the breakfast the slider retunes, minutes since scenario start.
const BREAKFAST_START_MIN: f64 = 360.0;
/// Playground scenario length.
const DAY_HOURS: f64 = 24.0;

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
    let req: DayRequest = serde_json::from_str(request).map_err(js_err)?;
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
    let (glucose, tir, tbr, tar) = report(&trace.reading_mmol_per_l);
    let result = DayResult {
        t_min: trace.t_min,
        glucose_mg_dl: glucose,
        bolus_u_breakfast,
        tir_pct: tir,
        tbr_pct: tbr,
        tar_pct: tar,
        meals: cfg.meals.iter().map(|m| [m.start_min, m.carbs_g]).collect(),
    };
    serde_json::to_string(&result).map_err(js_err)
}

fn missing(what: &str) -> JsValue {
    JsValue::from_str(&format!("realistic_day: no {what} at 06:00"))
}

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Sensor readings in mg/dL plus the report shares (tir, tbr, tar) for a day.
fn report(readings_mmol_per_l: &[f64]) -> (Vec<f64>, f64, f64, f64) {
    let readings = mg_dl_series(readings_mmol_per_l);
    let bands = range_bands_pct(&readings);
    (
        readings.iter().map(|g| g.0).collect(),
        bands[2],
        bands[0] + bands[1],
        bands[3] + bands[4],
    )
}

#[derive(Deserialize)]
pub struct ScenarioRequest {
    pub seed: u64,
    /// Auto mode on; off runs the fixed resting basal (hypo suspension stays).
    pub controller: bool,
    /// What-if multiplier on the nominal insulin sensitivities.
    pub resistance: f64,
    pub admit_mg_dl: f64,
    pub events: Vec<ScenarioEvent>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScenarioEvent {
    Meal { min: f64, g: f64 },
    Bolus { min: f64, u: f64 },
}

#[derive(Serialize, Deserialize)]
pub struct ScenarioResult {
    pub t_min: Vec<f64>,
    pub glucose_mg_dl: Vec<f64>,
    pub meals: Vec<[f64; 2]>,
    pub boluses: Vec<[f64; 2]>,
    pub tir_pct: f64,
    pub tbr_pct: f64,
    pub tar_pct: f64,
    pub total_bolus_u: f64,
}

/// Run a reader-composed day through the sim's two existing arms.
#[wasm_bindgen]
pub fn run_scenario(request: &str) -> Result<String, JsValue> {
    console_error_panic_hook::set_once();
    let req: ScenarioRequest = serde_json::from_str(request).map_err(js_err)?;
    serde_json::to_string(&run_scenario_inner(req)).map_err(js_err)
}

fn run_scenario_inner(req: ScenarioRequest) -> ScenarioResult {
    let mut subject = VirtualSubject::nominal(W10_WEIGHT_KG);
    subject.sit_per_mu_l *= req.resistance;
    subject.sid_per_mu_l *= req.resistance;
    subject.sie_per_mu_l *= req.resistance;
    let mut cfg = SimConfig {
        subject,
        use_controller: req.controller,
        admit_glucose_mg_per_dl: req.admit_mg_dl,
        duration_hours: DAY_HOURS,
        // One knob drives every noise source, so the seed pins the whole run.
        sensor_seed: req.seed,
        pump_seed: req.seed,
        variability_seed: req.seed,
        ..SimConfig::default()
    };
    for event in req.events {
        match event {
            ScenarioEvent::Meal { min, g } => cfg.meals.push(Meal {
                start_min: min,
                carbs_g: g,
                announced: true,
            }),
            ScenarioEvent::Bolus { min, u } => cfg.boluses.push(Bolus {
                start_min: min,
                units: u,
            }),
        }
    }
    let trace = simulate(&cfg);
    let (glucose, tir, tbr, tar) = report(&trace.reading_mmol_per_l);
    ScenarioResult {
        t_min: trace.t_min,
        glucose_mg_dl: glucose,
        meals: cfg.meals.iter().map(|m| [m.start_min, m.carbs_g]).collect(),
        boluses: cfg.boluses.iter().map(|b| [b.start_min, b.units]).collect(),
        tir_pct: tir,
        tbr_pct: tbr,
        tar_pct: tar,
        total_bolus_u: cfg.boluses.iter().fold(0.0, |acc, b| acc + b.units),
    }
}

#[derive(Deserialize, Serialize)]
pub struct SuggestRequest {
    pub cr_g_per_u: f64,
    pub isf_mg_dl_per_u: f64,
    pub target_mg_dl: f64,
    pub at_min: f64,
    pub carbs_g: f64,
    /// `None`: the calculator sees no glucose, so the correction is 0.
    pub glucose_now_mg_dl: Option<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct SuggestResult {
    pub meal_u: f64,
    pub correction_u: f64,
    pub total_u: f64,
}

/// The bolus calculator with the reader's three dials; no insulin on board.
#[wasm_bindgen]
pub fn suggest_bolus(request: &str) -> Result<String, JsValue> {
    console_error_panic_hook::set_once();
    let req: SuggestRequest = serde_json::from_str(request).map_err(js_err)?;
    let input = ProfileInput {
        carb_ratio: vec![Block::all_day(req.cr_g_per_u)],
        correction_factor: vec![Block::all_day(req.isf_mg_dl_per_u)],
        bolus_target: vec![Block::all_day(req.target_mg_dl)],
        auto_target: None,
        insulin_action_hours: 4.0,
        max_bolus_u: 30.0,
        min_glucose_mg_dl: 79.0,
        weight_kg: W10_WEIGHT_KG.round(),
        total_daily_dose_u: VirtualSubject::nominal(W10_WEIGHT_KG).walsh_tdd_u().round(),
    };
    let profile = Profile::new(input).map_err(js_err)?;
    let at = NaiveTime::MIN + TimeDelta::minutes(req.at_min.round() as i64);
    let s = suggest(
        profile.bolus(),
        at,
        Grams(req.carbs_g),
        req.glucose_now_mg_dl.map(MgDl),
        Units(0.0),
    )
    .map_err(js_err)?;
    let result = SuggestResult {
        meal_u: s.meal.0,
        correction_u: s.correction.0,
        total_u: s.total.0,
    };
    serde_json::to_string(&result).map_err(js_err)
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

    fn meal(min: f64, g: f64) -> ScenarioEvent {
        ScenarioEvent::Meal { min, g }
    }

    fn bolus(min: f64, u: f64) -> ScenarioEvent {
        ScenarioEvent::Bolus { min, u }
    }

    fn day(controller: bool, resistance: f64, events: Vec<ScenarioEvent>) -> ScenarioResult {
        run_scenario_inner(ScenarioRequest {
            seed: 1,
            controller,
            resistance,
            admit_mg_dl: 100.0,
            events,
        })
    }

    fn suggest_now(req: SuggestRequest) -> SuggestResult {
        let json = serde_json::to_string(&req).unwrap();
        serde_json::from_str(&suggest_bolus(&json).unwrap()).unwrap()
    }

    /// Three meals with boluses everywhere but breakfast (preset 1).
    fn preset_events() -> Vec<ScenarioEvent> {
        vec![
            meal(360.0, 60.0),
            meal(600.0, 50.0),
            meal(840.0, 70.0),
            bolus(600.0, 4.5),
            bolus(840.0, 6.5),
        ]
    }

    fn peak(v: &[f64]) -> f64 {
        v.iter().cloned().fold(f64::NAN, f64::max)
    }

    fn trough(v: &[f64]) -> f64 {
        v.iter().cloned().fold(f64::INFINITY, f64::min)
    }

    #[test]
    fn scenario_shape_matches_the_contract() {
        let day = day(true, 1.0, preset_events());
        assert_eq!(day.t_min.len(), 96);
        assert_eq!(day.meals, vec![[360.0, 60.0], [600.0, 50.0], [840.0, 70.0]]);
        assert_eq!(day.boluses, vec![[600.0, 4.5], [840.0, 6.5]]);
        assert_eq!(day.total_bolus_u, 11.0);
    }

    #[test]
    fn auto_mode_answers_resistance_by_overdelivering() {
        // The fixed-model loop sees the higher readings and gives more
        // insulin than the resistant body needs; the mean drops, it does
        // not rise (measured 135.9 -> 103.1 at r=1.5, seed 1).
        let nominal = day(true, 1.0, preset_events());
        let sick = day(true, 1.5, preset_events());
        assert!(mean(&sick.glucose_mg_dl) + 10.0 < mean(&nominal.glucose_mg_dl));
    }

    #[test]
    fn treating_the_meal_lowers_the_peak() {
        let untreated = day(false, 1.0, vec![meal(360.0, 60.0)]);
        assert!(peak(&untreated.glucose_mg_dl) > 180.0);
        // 5.5 U is what the calculator suggests for 60 g at cr 11.
        let treated = day(false, 1.0, vec![meal(360.0, 60.0), bolus(360.0, 5.5)]);
        assert!(peak(&treated.glucose_mg_dl) < peak(&untreated.glucose_mg_dl));
    }

    #[test]
    fn a_standalone_bolus_pushes_glucose_down() {
        let day = day(false, 1.0, vec![bolus(360.0, 6.0)]);
        assert!(trough(&day.glucose_mg_dl) < 85.0);
    }

    #[test]
    fn auto_mode_covers_the_events_better_than_basal_only() {
        let open = day(false, 1.0, preset_events());
        let closed = day(true, 1.0, preset_events());
        assert!(closed.tir_pct >= open.tir_pct);
    }

    #[test]
    fn suggestion_follows_the_calculator() {
        let req = |glucose: Option<f64>| SuggestRequest {
            cr_g_per_u: 11.0,
            isf_mg_dl_per_u: 50.0,
            target_mg_dl: 100.0,
            at_min: 360.0,
            carbs_g: 60.0,
            glucose_now_mg_dl: glucose,
        };
        let plain = suggest_now(req(None));
        assert_eq!(plain.total_u, 5.5);
        assert_eq!(plain.correction_u, 0.0);
        let high = suggest_now(req(Some(250.0)));
        assert_eq!(high.correction_u, 3.0);
        assert_eq!(high.total_u, 8.5);
    }
}
