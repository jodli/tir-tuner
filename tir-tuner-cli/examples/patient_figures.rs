//! Curve data for the patient pages, from the virtual patient.
//!
//! `cargo run -p tir-tuner-cli --example patient_figures > docs/patient/_data/figures.json`
//!
//! Every figure is a run of the body, sensor and controller models. The
//! scenarios (meals, doses, seeds) are made up for the pages.

use serde_json::{Value, json};
use tir_tuner_body::derivative::gut;
use tir_tuner_body::pump::Pump;
use tir_tuner_body::solver::{DT_MIN, rest_state, step};
use tir_tuner_body::state::BodyState;
use tir_tuner_body::subject::{VirtualSubject, W10_WEIGHT_KG};
use tir_tuner_cgm::{CgmSensor, SAMPLE_INTERVAL_MIN, SensorModel};
use tir_tuner_cli::engine::{SimConfig, realistic_day, simulate};
use tir_tuner_common::metrics::{
    TAR_VERY_HIGH_MG_DL, TIR_HIGH_MG_DL, TIR_LOW_MG_DL, range_bands_pct,
};
use tir_tuner_common::units::{
    MG_PER_DL_PER_MMOL_PER_L, MIN_PER_HOUR, MMOL_PER_GRAM_CHO, MU_PER_UNIT, MgDl, MmolL,
    mg_dl_series,
};

const DAYS: u64 = 14;
const REST_MG_DL: f64 = 100.0;
/// Model sample interval for the short figures (min).
const EVERY_MIN: f64 = 5.0;

/// Two decimals is finer than any figure draws.
fn series(v: &[f64]) -> Value {
    v.iter().map(|x| (x * 100.0).round() / 100.0).collect()
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let pos = p * (sorted.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

/// The realistic day `DAYS` times with fresh seeds: the readings of each day.
fn days(use_controller: bool) -> Vec<Vec<MgDl>> {
    (1..=DAYS)
        .map(|seed| {
            let cfg = SimConfig {
                use_controller,
                ..realistic_day(seed)
            };
            mg_dl_series(&simulate(&cfg).reading_mmol_per_l)
        })
        .collect()
}

/// Percentile curves across days, one point per sample.
fn percentiles(runs: &[Vec<MgDl>], ps: &[(&str, f64)]) -> Value {
    let curves: Vec<Vec<f64>> = (0..runs[0].len())
        .map(|i| {
            let mut col: Vec<f64> = runs.iter().map(|r| r[i].0).collect();
            col.sort_by(f64::total_cmp);
            ps.iter().map(|&(_, p)| percentile(&col, p)).collect()
        })
        .collect();
    ps.iter()
        .enumerate()
        .map(|(k, &(name, _))| {
            let c: Vec<f64> = curves.iter().map(|row| row[k]).collect();
            (name.to_string(), series(&c))
        })
        .collect()
}

fn bands(runs: &[Vec<MgDl>]) -> Value {
    series(&range_bands_pct(&runs.concat()))
}

/// The nominal subject from rest at 100 mg/dL for `hours`, sampled every
/// `EVERY_MIN`. `bolus_u` and `carbs_g` land at t=0, `basal` scales the
/// resting basal.
fn run(
    hours: f64,
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
    let per_sample = (EVERY_MIN / DT_MIN).round() as usize;
    let total = (hours * MIN_PER_HOUR / DT_MIN).round() as usize;
    for k in 0..=total {
        if k % per_sample == 0 {
            sample(&s, &state);
        }
        let u = (rate / MIN_PER_HOUR + pump.bolus_step(DT_MIN) / DT_MIN) * MU_PER_UNIT;
        state = step(&s, &state, u, DT_MIN);
    }
}

fn glucose(hours: f64, basal: f64, bolus_u: f64, carbs_g: f64) -> Value {
    let mut v = Vec::new();
    run(hours, basal, bolus_u, carbs_g, |s, st| {
        v.push(MmolL(st.plasma_glucose(s)).to_mg_dl().0)
    });
    series(&v)
}

type Effect = (&'static str, fn(&BodyState) -> f64);

/// Each effect of one bolus as a share of its own peak (%), above rest.
fn insulin() -> Value {
    let s = VirtualSubject::nominal(W10_WEIGHT_KG);
    let rest = rest_state(&s, MgDl(REST_MG_DL).to_mmol_l().0);
    let effects: [Effect; 4] = [
        ("blood", |st| st.i),
        ("liver", |st| st.x3),
        ("transport", |st| st.x1),
        ("disposal", |st| st.x2),
    ];
    let mut cols = vec![Vec::new(); effects.len()];
    run(8.0, 1.0, 5.0, 0.0, |_, st| {
        for (c, (_, f)) in cols.iter_mut().zip(&effects) {
            c.push(f(st) - f(&rest));
        }
    });
    let mut out = json!({ "dt_h": EVERY_MIN / MIN_PER_HOUR });
    for (c, (name, _)) in cols.iter().zip(&effects) {
        let peak = c.iter().copied().fold(f64::MIN, f64::max);
        let pct: Vec<f64> = c.iter().map(|x| 100.0 * x / peak).collect();
        out[*name] = series(&pct);
    }
    out
}

fn main() {
    let dt_h = EVERY_MIN / MIN_PER_HOUR;
    let auto = days(true);
    let basal = days(false);
    let day = realistic_day(1);
    let meals: Vec<[f64; 2]> = day
        .meals
        .iter()
        .map(|m| [m.start_min / MIN_PER_HOUR, m.carbs_g])
        .collect();

    let appearance = |carbs: f64| {
        let mut v = Vec::new();
        run(6.0, 1.0, 0.0, carbs, |s, st| {
            v.push(gut(s, st.g2).1 * s.weight_kg * MIN_PER_HOUR / MMOL_PER_GRAM_CHO);
        });
        series(&v)
    };

    let meal_bolus = VirtualSubject::nominal(W10_WEIGHT_KG).meal_bolus_u(60.0);
    let (mut blood, mut tissue, mut reading) = (Vec::new(), Vec::new(), Vec::new());
    let mut sensor = CgmSensor::new(SensorModel::Breton2008, 7);
    let per_reading = (SAMPLE_INTERVAL_MIN / EVERY_MIN).round() as usize;
    run(6.0, 1.0, meal_bolus, 60.0, |s, st| {
        blood.push(MmolL(st.plasma_glucose(s)).to_mg_dl().0);
        tissue.push(MmolL(st.c).to_mg_dl().0);
        if (blood.len() - 1) % per_reading == 0 {
            reading.push(sensor.read(MmolL(st.c)).0);
        }
    });

    let out = json!({
        "glucose": {
            "low": TIR_LOW_MG_DL,
            "high": TIR_HIGH_MG_DL,
            "very_high": TAR_VERY_HIGH_MG_DL,
            "mg_per_mmol": MG_PER_DL_PER_MMOL_PER_L,
        },
        "day": {
            "dt_h": SAMPLE_INTERVAL_MIN / MIN_PER_HOUR,
            "auto": percentiles(&auto, &[("p5", 0.05), ("p25", 0.25), ("p50", 0.5), ("p75", 0.75), ("p95", 0.95)]),
            "basal": percentiles(&basal, &[("p50", 0.5)]),
            "auto_bins": bands(&auto),
            "basal_bins": bands(&basal),
            "meals": meals,
            "days": DAYS,
        },
        "fasting": {
            "dt_h": dt_h,
            "basal": glucose(8.0, 1.0, 0.0, 0.0),
            "none": glucose(8.0, 0.0, 0.0, 0.0),
        },
        "gut": {
            "dt_h": dt_h,
            "g20": appearance(20.0),
            "g50": appearance(50.0),
            "g100": appearance(100.0),
        },
        "insulin": insulin(),
        "dosing": {
            "dt_h": dt_h,
            "weak": glucose(6.0, 1.0, 0.7 * meal_bolus, 60.0),
            "right": glucose(6.0, 1.0, meal_bolus, 60.0),
            "strong": glucose(6.0, 1.0, 1.3 * meal_bolus, 60.0),
        },
        "sensor": {
            "dt_h": dt_h,
            "blood": series(&blood),
            "tissue": series(&tissue),
            "reading_dt_h": SAMPLE_INTERVAL_MIN / MIN_PER_HOUR,
            "reading": series(&reading),
        },
    });
    println!("{out}");
}
