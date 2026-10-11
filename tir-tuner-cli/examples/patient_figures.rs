//! Curve data for the patient pages, from the virtual patient.
//!
//! `cargo run -p tir-tuner-cli --example patient_figures > docs/patient/_data/figures.json`
//!
//! Every figure is a run of the body, sensor and controller models. The
//! scenarios (meals, doses, seeds) are made up for the pages.

use std::fmt::Write;

use tir_tuner_body::derivative::gut;
use tir_tuner_body::pump::Pump;
use tir_tuner_body::solver::{DT_MIN, rest_state, step};
use tir_tuner_body::state::BodyState;
use tir_tuner_body::subject::{VirtualSubject, W10_WEIGHT_KG};
use tir_tuner_cgm::{CgmSensor, SensorModel};
use tir_tuner_cli::engine::{Meal, SimConfig, icr_boluses, simulate};
use tir_tuner_common::units::{MIN_PER_HOUR, MMOL_PER_GRAM_CHO, MU_PER_UNIT, MgDl, MmolL};

const DAYS: u64 = 14;
const REST_MG_DL: f64 = 100.0;
/// The engine's tested realistic day: admit glucose and meal boluses at 80%
/// of the Walsh ratio.
const ADMIT_MG_DL: f64 = 140.0;
const BOLUS_SHARE: f64 = 0.8;
/// Its meals: clock hour and grams.
const MEALS: [(f64, f64); 4] = [(6.0, 60.0), (10.0, 50.0), (14.0, 70.0), (19.0, 60.0)];

fn mg(mmol: f64) -> f64 {
    MmolL(mmol).to_mg_dl().0
}

fn arr(out: &mut String, key: &str, v: &[f64]) {
    let body: Vec<String> = v.iter().map(|x| format!("{:.2}", x)).collect();
    write!(out, "\"{key}\":[{}]", body.join(",")).unwrap();
}

fn obj(fields: &[(&str, &[f64])]) -> String {
    let mut s = String::from("{");
    for (i, (k, v)) in fields.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        arr(&mut s, k, v);
    }
    s.push('}');
    s
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let pos = p * (sorted.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

/// Very low, low, in range, high, very high, in percent (Battelino's whole
/// mg/dL edges).
fn bins(readings: &[f64]) -> [f64; 5] {
    let mut n = [0.0; 5];
    for &r in readings {
        let r = MgDl(r).0.round();
        let i = match r {
            r if r < 54.0 => 0,
            r if r < 70.0 => 1,
            r if r <= 180.0 => 2,
            r if r <= 250.0 => 3,
            _ => 4,
        };
        n[i] += 1.0;
    }
    n.map(|c| 100.0 * c / readings.len() as f64)
}

/// The engine's tested realistic day `DAYS` times with fresh seeds:
/// percentile curves and bins.
fn days(use_controller: bool) -> (Vec<f64>, String, [f64; 5]) {
    let meals: Vec<Meal> = MEALS
        .iter()
        .map(|&(h, g)| Meal {
            start_min: h * MIN_PER_HOUR,
            carbs_g: g,
            announced: true,
        })
        .collect();
    let base = SimConfig::default();
    let boluses = icr_boluses(&base.subject, &meals, BOLUS_SHARE);
    let runs: Vec<Vec<f64>> = (0..DAYS)
        .map(|k| {
            let cfg = SimConfig {
                duration_hours: 24.0,
                admit_glucose_mg_per_dl: ADMIT_MG_DL,
                meals: meals.clone(),
                boluses: boluses.clone(),
                use_controller,
                sensor_seed: 100 + k,
                pump_seed: 200 + k,
                variability_seed: 300 + k,
                ..base.clone()
            };
            simulate(&cfg)
                .reading_mmol_per_l
                .iter()
                .map(|&r| mg(r))
                .collect()
        })
        .collect();
    let n = runs[0].len();
    let t_h: Vec<f64> = (0..n).map(|i| i as f64 * 0.25).collect();
    let ps = [0.05, 0.25, 0.5, 0.75, 0.95];
    let mut curves = vec![Vec::with_capacity(n); ps.len()];
    for i in 0..n {
        let mut col: Vec<f64> = runs.iter().map(|r| r[i]).collect();
        col.sort_by(f64::total_cmp);
        for (c, &p) in curves.iter_mut().zip(&ps) {
            c.push(percentile(&col, p));
        }
    }
    let all: Vec<f64> = runs.concat();
    let json = obj(&[
        ("p5", &curves[0]),
        ("p25", &curves[1]),
        ("p50", &curves[2]),
        ("p75", &curves[3]),
        ("p95", &curves[4]),
    ]);
    (t_h, json, bins(&all))
}

/// The nominal subject from rest at 100 mg/dL for `hours`, sampled every
/// `every_min`. `bolus_u` and `carbs_g` land at t=0, `basal` scales the
/// resting basal.
fn run(
    hours: f64,
    every_min: f64,
    basal: f64,
    bolus_u: f64,
    carbs_g: f64,
    mut sample: impl FnMut(&VirtualSubject, &BodyState),
) {
    let s = VirtualSubject::nominal(W10_WEIGHT_KG);
    let mut state = rest_state(&s, MgDl(REST_MG_DL).to_mmol_l().0);
    let rate = basal * s.rate_for_insulin(state.i);
    let mut pump = Pump::new(0, 0.0);
    pump.bolus(bolus_u);
    state.eat(&s, carbs_g, 1.0);
    let per_sample = (every_min / DT_MIN).round() as usize;
    let total = (hours * MIN_PER_HOUR / DT_MIN).round() as usize;
    for k in 0..=total {
        if k % per_sample == 0 {
            sample(&s, &state);
        }
        let u = (rate / MIN_PER_HOUR + pump.bolus_step(DT_MIN) / DT_MIN) * MU_PER_UNIT;
        state = step(&s, &state, u, DT_MIN);
    }
}

fn times(hours: f64, every_min: f64) -> Vec<f64> {
    let n = (hours * MIN_PER_HOUR / every_min).round() as usize;
    (0..=n)
        .map(|i| i as f64 * every_min / MIN_PER_HOUR)
        .collect()
}

fn glucose(hours: f64, basal: f64, bolus_u: f64, carbs_g: f64) -> Vec<f64> {
    let mut v = Vec::new();
    run(hours, 5.0, basal, bolus_u, carbs_g, |s, st| {
        v.push(mg(st.plasma_glucose(s)))
    });
    v
}

fn main() {
    let s = VirtualSubject::nominal(W10_WEIGHT_KG);
    let mut out = String::from("{");

    let (t_day, auto, auto_bins) = days(true);
    let (_, basal, basal_bins) = days(false);
    write!(out, "\"day\":{{").unwrap();
    arr(&mut out, "t_h", &t_day);
    write!(out, ",\"auto\":{auto},\"basal\":{basal},").unwrap();
    arr(&mut out, "auto_bins", &auto_bins);
    out.push(',');
    arr(&mut out, "basal_bins", &basal_bins);
    let meals: Vec<String> = MEALS.iter().map(|(h, g)| format!("[{h},{g}]")).collect();
    write!(out, ",\"meals\":[{}],\"days\":{DAYS}}},", meals.join(",")).unwrap();

    let fasting = obj(&[
        ("t_h", &times(8.0, 5.0)),
        ("basal", &glucose(8.0, 1.0, 0.0, 0.0)),
        ("none", &glucose(8.0, 0.0, 0.0, 0.0)),
    ]);
    write!(out, "\"fasting\":{fasting},").unwrap();

    let appearance = |carbs: f64| {
        let mut v = Vec::new();
        run(6.0, 5.0, 1.0, 0.0, carbs, |s, st| {
            let g_per_h = gut(s, st.g2).1 * s.weight_kg * MIN_PER_HOUR / MMOL_PER_GRAM_CHO;
            v.push(g_per_h);
        });
        v
    };
    let gut_json = obj(&[
        ("t_h", &times(6.0, 5.0)),
        ("g20", &appearance(20.0)),
        ("g50", &appearance(50.0)),
        ("g100", &appearance(100.0)),
    ]);
    write!(out, "\"gut\":{gut_json},").unwrap();

    let rest = rest_state(&s, MgDl(REST_MG_DL).to_mmol_l().0);
    let mut cols = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    run(8.0, 5.0, 1.0, 5.0, 0.0, |_, st| {
        cols[0].push(st.i - rest.i);
        cols[1].push(st.x1 - rest.x1);
        cols[2].push(st.x2 - rest.x2);
        cols[3].push(st.x3 - rest.x3);
    });
    let norm = |v: &[f64]| {
        let peak = v.iter().cloned().fold(f64::MIN, f64::max);
        v.iter().map(|x| x / peak).collect::<Vec<_>>()
    };
    let insulin = obj(&[
        ("t_h", &times(8.0, 5.0)),
        ("blood", &norm(&cols[0])),
        ("liver", &norm(&cols[3])),
        ("transport", &norm(&cols[1])),
        ("disposal", &norm(&cols[2])),
    ]);
    write!(out, "\"insulin\":{insulin},").unwrap();

    let meal_bolus = s.meal_bolus_u(60.0);
    let dosing = obj(&[
        ("t_h", &times(6.0, 5.0)),
        ("weak", &glucose(6.0, 1.0, 0.7 * meal_bolus, 60.0)),
        ("right", &glucose(6.0, 1.0, meal_bolus, 60.0)),
        ("strong", &glucose(6.0, 1.0, 1.3 * meal_bolus, 60.0)),
    ]);
    write!(out, "\"dosing\":{dosing},").unwrap();

    let (mut blood, mut tissue, mut reading) = (Vec::new(), Vec::new(), Vec::new());
    let mut sensor = CgmSensor::new(SensorModel::Breton2008, 7);
    let mut k = 0;
    run(6.0, 5.0, 1.0, meal_bolus, 60.0, |s, st| {
        blood.push(mg(st.plasma_glucose(s)));
        tissue.push(mg(st.c));
        if k % 3 == 0 {
            reading.push(sensor.read(MmolL(st.c)).0);
        }
        k += 1;
    });
    let sensor_json = obj(&[
        ("t_h", &times(6.0, 5.0)),
        ("blood", &blood),
        ("tissue", &tissue),
        ("reading_t_h", &times(6.0, 15.0)),
        ("reading", &reading),
    ]);
    write!(out, "\"sensor\":{sensor_json}}}").unwrap();
    println!("{out}");
}
