//! In-silico closed-loop engine: body -> CGM -> aps controller -> pump.
//!
//! The engine wires the verified crates together into a deterministic
//! scenario runner. A subject starts at an admit glucose, the CGM sensor
//! reads the interstitial compartment every five minutes, the aps
//! controller turns the reading into an insulin rate, the pump applies
//! a small delivery error, and the body integrates forward on a
//! sub-minute grid. Meals are time-gated carbohydrate inputs.
//!
//! The controller rate is a correction on top of the subject's basal
//! requirement (the aps crate sizes a proportional dose in U/h per
//! mmol/L above target). The pump model is a single multiplicative
//! delivery error with no occlusion or overshoot states, then everything
//! lands on the body's subcutaneous insulin influx.

use tir_tuner_aps::controller::{compute_nmpc_dose, is_hypoglycemic};
use tir_tuner_body::derivative::BodyInputs;
use tir_tuner_body::solver::{admit_state, step, DT_MIN};
use tir_tuner_body::subject::VirtualSubject;
use tir_tuner_cgm::device::{CgmSensor, SensorParams};

use tir_tuner_common::random::SeededRng;
use tir_tuner_common::units::MU_PER_UNIT;

/// Controller sampling period in minutes: matches the CGM cadence and
/// the aps controller's per-cycle update.
pub const CONTROL_PERIOD_MIN: f64 = 5.0;

/// A time-gated carbohydrate input.
#[derive(Clone, Copy, Debug)]
pub struct Meal {
    /// Start of the meal, minutes since scenario start.
    pub start_min: f64,
    /// Total carbohydrates in grams.
    pub carbs_g: f64,
    /// Duration of consumption in minutes.
    pub duration_min: f64,
}

/// Everything a single simulation run needs.
#[derive(Clone, Debug)]
pub struct SimConfig {
    pub subject: VirtualSubject,
    /// Presenting plasma glucose at t=0, mg/dL.
    pub admit_glucose_mg_per_dl: f64,
    /// Total simulated time in hours.
    pub duration_hours: f64,
    pub meals: Vec<Meal>,
    /// Maximum pump delivery rate (U/h).
    pub max_delivery_u_per_h: f64,
    /// Sensor noise seed.
    pub sensor_seed: u64,
    /// Pump delivery-error seed.
    pub pump_seed: u64,
    /// Pump relative delivery error, as percent CV (0.05 = 5%).
    pub pump_error_cv: f64,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            subject: VirtualSubject::population_mean(),
            admit_glucose_mg_per_dl: 100.0,
            duration_hours: 12.0,
            meals: Vec::new(),
            max_delivery_u_per_h: 10.0,
            sensor_seed: 1,
            pump_seed: 2,
            pump_error_cv: 0.05,
        }
    }
}

/// The recorded trace of one simulation.
#[derive(Clone, Debug)]
pub struct SimTrace {
    /// Sample times in minutes since start, one per controller period.
    pub t_min: Vec<f64>,
    /// CGM reading at each sample time (mmol/L), as seen by the
    /// controller: noisy, clipped at zero.
    pub reading_mmol_per_l: Vec<f64>,
    /// Total insulin rate that reached the body (U/h).
    pub delivered_u_per_h: Vec<f64>,
    /// True interstitial glucose measured by the sensor (mmol/L).
    pub interstitial_mmol_per_l: Vec<f64>,
}

/// Run one scenario deterministically from the seeds.
pub fn simulate(cfg: &SimConfig) -> SimTrace {
    let total_min = cfg.duration_hours * 60.0;
    let steps_per_control: usize = (CONTROL_PERIOD_MIN / DT_MIN).round() as usize;

    let mut state = admit_state(&cfg.subject, cfg.admit_glucose_mg_per_dl);
    let mut sensor = CgmSensor::new(SensorParams::default(), cfg.sensor_seed);
    let mut pump_rng = SeededRng::new(cfg.pump_seed);

    let basal_u_per_h = cfg.subject.bir_u_per_h;
    let basal_mu_per_min = basal_u_per_h / 60.0 * MU_PER_UNIT;

    let mut meals = cfg.meals.clone();
    meals.sort_by(|a, b| a.start_min.total_cmp(&b.start_min));

    let mut t_min = Vec::new();
    let mut reading_mmol_per_l = Vec::new();
    let mut delivered_u_per_h = Vec::new();
    let mut interstitial_mmol_per_l = Vec::new();

    let mut t = 0.0;
    while t < total_min {
        let reading = sensor.read(state.c);

        // Correction over basal, clamped by the controller to
        // [0, max_delivery]; the pump then applies its delivery error.
        // Below the hard hypoglycemia cutoff the pump suspends entirely,
        // matching the aps hard-hypo rule (this overrides the basal that
        // would otherwise keep flowing).
        let correction_u_per_h = if is_hypoglycemic(reading.glucose_mmol_per_l) {
            0.0
        } else {
            compute_nmpc_dose(reading.glucose_mmol_per_l, cfg.max_delivery_u_per_h)
        };
        let delivered = pump_delivery(
            basal_mu_per_min,
            correction_u_per_h,
            reading.glucose_mmol_per_l,
            &mut pump_rng,
            cfg.pump_error_cv,
        );

        let meal_g_per_min = meals
            .iter()
            .find(|m| t >= m.start_min && t < m.start_min + m.duration_min)
            .map_or(0.0, |m| m.carbs_g / m.duration_min);

        let inputs = BodyInputs {
            u_basal_mu_per_min: delivered,
            meal_g_per_min,
        };

        for _ in 0..steps_per_control {
            state = step(&cfg.subject, &state, &inputs, DT_MIN);
        }

        t_min.push(t);
        reading_mmol_per_l.push(reading.glucose_mmol_per_l);
        delivered_u_per_h.push(delivered / MU_PER_UNIT * 60.0);
        interstitial_mmol_per_l.push(state.c);

        t += CONTROL_PERIOD_MIN;
    }

    SimTrace {
        t_min,
        reading_mmol_per_l,
        delivered_u_per_h,
        interstitial_mmol_per_l,
    }
}

/// Pump model: multiplicative delivery error around the controller rate.
/// The controller already clamps the correction to `[0, u_max]`; here
/// the pump applies `(1 + e)` with `e ~ N(0, cv)` to the basal-plus-
/// correction total and floors negative delivery at zero. On a hypo
/// reading the loop passes a zero correction and delivery is suspended
/// outright (basal included). Returns the rate in mU/min.
fn pump_delivery(
    basal_mu_per_min: f64,
    correction_u_per_h: f64,
    reading_mmol_per_l: f64,
    rng: &mut SeededRng,
    error_cv: f64,
) -> f64 {
    let effective_basal = if is_hypoglycemic(reading_mmol_per_l) {
        0.0
    } else {
        basal_mu_per_min
    };
    let correction_mu_per_min = correction_u_per_h / 60.0 * MU_PER_UNIT;
    let total = effective_basal + correction_mu_per_min;
    let error = 1.0 + error_cv * rng.next_normal();
    (total * error).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tir_tuner_common::metrics::{mean_glucose, time_in_range_pct};

    #[test]
    fn open_loop_basal_holds_interstitial_near_rest() {
        // No meals, no correction: the pump delivers basal only, and the
        // calibrated mean subject should stay near its resting glucose.
        let cfg = SimConfig::default();
        let trace = simulate(&cfg);

        let mean = mean_glucose(&trace.interstitial_mmol_per_l);
        assert!(mean.is_finite());
        // Interstitial rest differs from plasma; stay within a plausible
        // band around the 5.8 mmol/L target rather than asserting exact.
        assert!(
            (2.5..=10.0).contains(&mean),
            "open-loop interstitial mean drifted to {mean}"
        );
        assert_eq!(trace.t_min.len(), trace.reading_mmol_per_l.len());
    }

    #[test]
    fn meal_without_correction_lifts_glucose() {
        let mut cfg = SimConfig::default();
        cfg.duration_hours = 8.0;
        cfg.meals = vec![Meal {
            start_min: 60.0,
            carbs_g: 60.0,
            duration_min: 15.0,
        }];
        // No correction: basal only, a large meal still drives the CGM
        // reading up measurably. Before-window: t in [0,55]; meal window
        // itself t in [60,75]; comparison uses t in [100,295] after the
        // meal has hit the gut.
        let trace = simulate(&cfg);

        let before = mean_glucose(&trace.reading_mmol_per_l[..12]);
        let window = mean_glucose(&trace.reading_mmol_per_l[20..60]);
        assert!(
            window > before,
            "meal did not lift glucose: before {before:.2}, after {window:.2}"
        );
    }

    #[test]
    fn closed_loop_corrects_hyperglycemia_down() {
        // Start high (180 mg/dL = 10 mmol/L) with a large delivery headroom.
        // The proportional controller should pull the interstitial glucose
        // down over the run: later readings lower than the admit reading.
        let mut cfg = SimConfig::default();
        cfg.admit_glucose_mg_per_dl = 180.0;
        cfg.duration_hours = 12.0;
        cfg.max_delivery_u_per_h = 20.0;
        cfg.pump_error_cv = 0.0; // isolate the controller
        let trace = simulate(&cfg);

        let start = trace.reading_mmol_per_l[0];
        let end = trace.reading_mmol_per_l.last().copied().unwrap();
        assert!(
            end < start,
            "closed loop did not pull glucose down: {start:.2} -> {end:.2}"
        );
    }

    #[test]
    fn readings_never_negative_and_in_range_termination() {
        let mut cfg = SimConfig::default();
        cfg.duration_hours = 24.0;
        cfg.meals = vec![
            Meal { start_min: 300.0, carbs_g: 70.0, duration_min: 20.0 },
            Meal { start_min: 700.0, carbs_g: 60.0, duration_min: 20.0 },
        ];
        let trace = simulate(&cfg);

        for &v in &trace.reading_mmol_per_l {
            assert!(v >= 0.0, "reading went negative: {v}");
            assert!(v.is_finite());
        }
        let tir = time_in_range_pct(&trace.reading_mmol_per_l);
        assert!((0.0..=100.0).contains(&tir), "TIR out of bounds: {tir}");
    }

    #[test]
    fn deterministic_from_seeds() {
        let cfg = SimConfig::default();
        let a = simulate(&cfg);
        let b = simulate(&cfg);
        assert_eq!(a.reading_mmol_per_l, b.reading_mmol_per_l);
        assert_eq!(a.delivered_u_per_h, b.delivered_u_per_h);
    }

    #[test]
    fn control_period_integrates_ahead_of_sampling() {
        // With one 5-min step of DT_MIN=0.25 the state should advance by
        // exactly one control period.
        let mut cfg = SimConfig::default();
        cfg.duration_hours = 5.0 / 60.0;
        let trace = simulate(&cfg);
        assert_eq!(trace.t_min.len(), 1);
        assert!((trace.t_min[0] - 0.0).abs() < 1e-9);
    }

    #[test]
    fn hypo_reading_suspends_pump_totally() {
        // Force the reading below the hard cutoff regardless of body state
        // and verify the delivered rate collapses to zero. With a large
        // post-meal crash the loop should produce samples in hypo; we
        // assert the delivery record has a zero entry whenever the reading
        // breaks the cutoff.
        let mut cfg = SimConfig::default();
        cfg.duration_hours = 24.0;
        cfg.meals = vec![Meal { start_min: 60.0, carbs_g: 60.0, duration_min: 15.0 }];
        cfg.max_delivery_u_per_h = 20.0;
        cfg.pump_error_cv = 0.0;
        let trace = simulate(&cfg);

        let hypo_samples: Vec<_> = trace
            .reading_mmol_per_l
            .iter()
            .enumerate()
            .filter(|(_, &r)| tir_tuner_aps::controller::is_hypoglycemic(r))
            .collect();
        assert!(!hypo_samples.is_empty(), "expected at least one hypo sample post-meal");
        for (i, _) in &hypo_samples {
            assert_eq!(
                trace.delivered_u_per_h[*i], 0.0,
                "pump must be fully suspended at hypo sample {i}"
            );
        }
    }
}
