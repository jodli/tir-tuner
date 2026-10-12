#![warn(missing_docs)]
//! In-silico closed loop: body -> CGM -> aps controller -> pump.
//!
//! The subject starts at rest at the admit glucose, on the basal that holds
//! it there (W10 p7). Every control period the CGM reads the interstitial
//! compartment, the patent's IMM filter updates the controller's belief, the
//! NMPC picks a rate and the Auto mode variant adjusts it. The pump delivers
//! it with W10's error, and the body integrates meals and boluses at their
//! own minute. The controller hears of announced meals and every bolus.
//!
//! The belief model is built from the Auto mode's weight and TDD, not from
//! the plant subject; unlike CamAPS it doesn't learn. With
//! `use_controller` off the pump delivers the resting basal, the open-loop
//! arm.

use chrono::{NaiveTime, TimeDelta};
use tir_tuner_aps::AutoMode;
#[cfg(test)]
use tir_tuner_aps::controller::CONTROL_PERIOD_MIN;
use tir_tuner_aps::controller::{Mode, Nmpc, ROLLOUT_STEP_MIN};
use tir_tuner_aps::hovorka::{HovorkaParams, HovorkaState};
use tir_tuner_aps::kalman::Estimator;
use tir_tuner_body::pump::{DELIVERY_ERROR_CV, Pump};
use tir_tuner_body::solver::{DT_MIN, rest_state, step};
use tir_tuner_body::subject::{Oscillation, VirtualSubject, W10_WEIGHT_KG, meal_bio_factor};
use tir_tuner_cgm::{CgmSensor, SAMPLE_INTERVAL_MIN, SensorModel};

use tir_tuner_common::random::SeededRng;
use tir_tuner_common::units::{MIN_PER_HOUR, MU_PER_UNIT, MgDl, MmolL};

/// Carbohydrates eaten at one moment.
#[derive(Clone, Copy, Debug)]
pub struct Meal {
    /// Minutes since scenario start.
    pub start_min: f64,
    /// Grams of carbohydrate.
    pub carbs_g: f64,
    /// Entered in the app, so the controller knows it.
    pub announced: bool,
}

/// A bolus the user starts.
#[derive(Clone, Copy, Debug)]
pub struct Bolus {
    /// Minutes since scenario start.
    pub start_min: f64,
    /// Units programmed.
    pub units: f64,
}

/// Everything a single simulation run needs.
#[derive(Clone, Debug)]
#[allow(missing_docs)]
pub struct SimConfig {
    /// The virtual patient, at nominal parameters.
    pub subject: VirtualSubject,
    /// The user's Auto mode settings: personal target and the weight the
    /// controller's model assumes.
    pub auto: AutoMode,
    /// Auto mode variant for the whole run.
    pub mode: Mode,
    /// Time of day at t=0, for the personal target.
    pub start_time: NaiveTime,
    /// Plasma glucose at t=0 (mg/dL); the subject rests there.
    pub admit_glucose_mg_per_dl: f64,
    /// Total simulated time in hours.
    pub duration_hours: f64,
    pub meals: Vec<Meal>,
    pub boluses: Vec<Bolus>,
    pub nmpc: Nmpc,
    /// Run the closed loop; when false the pump delivers the resting basal
    /// (hypo suspension still active).
    pub use_controller: bool,
    /// W10's parameter oscillation and meal-to-meal bioavailability.
    pub variability: bool,
    pub sensor: SensorModel,
    pub sensor_seed: u64,
    pub pump_seed: u64,
    /// Seed for the oscillation phases and meal bioavailability.
    pub variability_seed: u64,
    /// Pump delivery error, as a CV.
    pub pump_error_cv: f64,
}

/// Auto mode without stored settings: default target, W10's weight, and the
/// TDD a well-dosed W10 subject would enter (Walsh eq 1 from its basal).
pub fn w10_auto() -> AutoMode {
    let tdd = VirtualSubject::nominal(W10_WEIGHT_KG).walsh_tdd_u();
    AutoMode::without_personal_target(W10_WEIGHT_KG, tdd)
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            subject: VirtualSubject::nominal(W10_WEIGHT_KG),
            auto: w10_auto(),
            mode: Mode::Standard,
            start_time: NaiveTime::MIN,
            admit_glucose_mg_per_dl: 100.0,
            duration_hours: 12.0,
            meals: Vec::new(),
            boluses: Vec::new(),
            nmpc: Nmpc::default(),
            use_controller: true,
            variability: true,
            sensor: SensorModel::Breton2008,
            sensor_seed: 1,
            pump_seed: 2,
            variability_seed: 3,
            pump_error_cv: DELIVERY_ERROR_CV,
        }
    }
}

/// The recorded trace of one simulation.
#[derive(Clone, Debug)]
pub struct SimTrace {
    /// Sample times in minutes since start, one per controller period.
    pub t_min: Vec<f64>,
    /// CGM reading at each sample time (mmol/L), as the controller sees it.
    pub reading_mmol_per_l: Vec<f64>,
    /// Insulin that reached the body over the period, boluses included (U/h).
    pub delivered_u_per_h: Vec<f64>,
    /// True interstitial glucose at the end of the period (mmol/L).
    pub interstitial_mmol_per_l: Vec<f64>,
    /// Basal (U/h) that holds the subject at rest at the admit glucose.
    pub resting_basal_u_per_h: f64,
}

/// The belief over one period: announced carbs and programmed boluses in
/// its first minute, the rate throughout.
fn propagate_period(
    params: &HovorkaParams,
    b: HovorkaState,
    rate_u_per_h: f64,
    bolus_u: f64,
    carbs_g: f64,
    steps: usize,
) -> HovorkaState {
    let mut b = b.step(
        params,
        rate_u_per_h + bolus_u * MIN_PER_HOUR / ROLLOUT_STEP_MIN,
        carbs_g / ROLLOUT_STEP_MIN,
        ROLLOUT_STEP_MIN,
    );
    for _ in 1..steps {
        b = b.step(params, rate_u_per_h, 0.0, ROLLOUT_STEP_MIN);
    }
    b
}

/// Boluses at the subject's Walsh ratio, scaled by `factor`, for the
/// announced meals.
pub fn icr_boluses(subject: &VirtualSubject, meals: &[Meal], factor: f64) -> Vec<Bolus> {
    meals
        .iter()
        .filter(|m| m.announced)
        .map(|m| Bolus {
            start_min: m.start_min,
            units: factor * subject.meal_bolus_u(m.carbs_g),
        })
        .collect()
}

/// Four meals bolused at 0.8 of the Walsh ratio, from 140 mg/dL. The tests
/// hold it in range and the patient pages show it.
pub fn realistic_day(seed: u64) -> SimConfig {
    let mut cfg = SimConfig {
        duration_hours: 24.0,
        admit_glucose_mg_per_dl: 140.0,
        meals: [(360.0, 60.0), (600.0, 50.0), (840.0, 70.0), (1140.0, 60.0)]
            .map(|(start_min, carbs_g)| Meal {
                start_min,
                carbs_g,
                announced: true,
            })
            .to_vec(),
        variability_seed: seed,
        pump_seed: seed + 100,
        sensor_seed: seed + 200,
        ..SimConfig::default()
    };
    cfg.boluses = icr_boluses(&cfg.subject, &cfg.meals, 0.8);
    cfg
}

/// Run one scenario deterministically from the seeds.
///
/// # Panics
///
/// If the control period isn't the sensor's sample interval: B08's error
/// correlation is per 15-min sample.
pub fn simulate(cfg: &SimConfig) -> SimTrace {
    let period_min = cfg.nmpc.step_min;
    assert_eq!(
        period_min, SAMPLE_INTERVAL_MIN,
        "sensor model needs 15-min samples"
    );
    let total_min = cfg.duration_hours * MIN_PER_HOUR;
    let body_steps = (period_min / DT_MIN).round() as usize;
    let belief_steps = (period_min / ROLLOUT_STEP_MIN).round() as usize;

    let admit = MgDl(cfg.admit_glucose_mg_per_dl).to_mmol_l().0;
    let mut state = rest_state(&cfg.subject, admit);
    let basal_u_per_h = cfg.subject.rate_for_insulin(state.i);
    let params = cfg.auto.model();
    let mut estimator = Estimator::new(&params, HovorkaState::basal(&params, admit));

    let mut sensor = CgmSensor::new(cfg.sensor, cfg.sensor_seed);
    let mut pump = Pump::new(cfg.pump_seed, cfg.pump_error_cv);
    let mut rng = SeededRng::new(cfg.variability_seed);
    let oscillation = cfg.variability.then(|| Oscillation::draw(&mut rng));
    let meals: Vec<(Meal, f64)> = cfg
        .meals
        .iter()
        .map(|&m| {
            (
                m,
                if cfg.variability {
                    meal_bio_factor(&mut rng)
                } else {
                    1.0
                },
            )
        })
        .collect();
    let within = |at: f64, start: f64, len: f64| at >= start && at < start + len;

    let mut trace = SimTrace {
        t_min: Vec::new(),
        reading_mmol_per_l: Vec::new(),
        delivered_u_per_h: Vec::new(),
        interstitial_mmol_per_l: Vec::new(),
        resting_basal_u_per_h: basal_u_per_h,
    };

    let mut t = 0.0;
    // The rate of the previous period, `u(t)` in the eq 9 effort term,
    // seeded from the controller's own basal, not the plant's.
    let mut last_rate_u_per_h = params.bir;
    while t < total_min {
        let reading = sensor.read(MmolL(state.c)).to_mmol_l().0;
        estimator.update(&params, reading);

        let personal = cfg
            .auto
            .target_mmol_l(cfg.start_time + TimeDelta::minutes(t as i64));
        let target = cfg.mode.target(personal);
        let rate_u_per_h = if cfg.use_controller {
            let nmpc = cfg
                .nmpc
                .dose(&params, estimator.state, target, last_rate_u_per_h);
            cfg.mode.rate(nmpc, reading, target, cfg.nmpc.u_max)
        } else if Mode::Standard.suspended(reading) {
            0.0
        } else {
            basal_u_per_h
        };
        last_rate_u_per_h = rate_u_per_h;
        let infused_u_per_h = pump.infusion(rate_u_per_h);

        let mut bolus_u = 0.0;
        for k in 0..body_steps {
            let tau = t + k as f64 * DT_MIN;
            let body = match &oscillation {
                Some(osc) => cfg.subject.oscillated(osc, tau),
                None => cfg.subject,
            };
            for (m, bio) in meals
                .iter()
                .filter(|(m, _)| within(m.start_min, tau, DT_MIN))
            {
                state.eat(&body, m.carbs_g, *bio);
            }
            for b in cfg
                .boluses
                .iter()
                .filter(|b| within(b.start_min, tau, DT_MIN))
            {
                pump.bolus(b.units);
            }
            let bolus_step_u = pump.bolus_step(DT_MIN);
            bolus_u += bolus_step_u;
            let u_mu_per_min =
                (infused_u_per_h / MIN_PER_HOUR + bolus_step_u / DT_MIN) * MU_PER_UNIT;
            state = step(&body, &state, u_mu_per_min, DT_MIN);
        }

        // The controller's view: announced carbs and programmed boluses in
        // the first minute of the period.
        let known_carbs: f64 = cfg
            .meals
            .iter()
            .filter(|m| m.announced && within(m.start_min, t, period_min))
            .map(|m| m.carbs_g)
            .sum();
        let known_bolus: f64 = cfg
            .boluses
            .iter()
            .filter(|b| within(b.start_min, t, period_min))
            .map(|b| b.units)
            .sum();
        estimator.predict(period_min, |b| {
            propagate_period(
                &params,
                b,
                rate_u_per_h,
                known_bolus,
                known_carbs,
                belief_steps,
            )
        });

        trace.t_min.push(t);
        trace.reading_mmol_per_l.push(reading);
        trace
            .delivered_u_per_h
            .push(infused_u_per_h + bolus_u * MIN_PER_HOUR / period_min);
        trace.interstitial_mmol_per_l.push(state.c);

        t += period_min;
    }
    trace
}

#[cfg(test)]
mod tests {
    use super::*;
    use tir_tuner_aps::hovorka::REST_MMOL_L;
    use tir_tuner_common::metrics::{mean_glucose, time_below_range_pct, time_in_range_pct};
    use tir_tuner_common::units::mg_dl_series;

    /// Metrics take mg/dL readings; the traces are mmol/L.
    fn mean_mmol(trace: &[f64]) -> f64 {
        mean_glucose(&mg_dl_series(trace)).to_mmol_l().0
    }

    fn tir_pct(trace: &[f64]) -> f64 {
        time_in_range_pct(&mg_dl_series(trace))
    }

    fn tbr_pct(trace: &[f64]) -> f64 {
        time_below_range_pct(&mg_dl_series(trace))
    }

    fn meal(start_min: f64, carbs_g: f64, announced: bool) -> Meal {
        Meal {
            start_min,
            carbs_g,
            announced,
        }
    }

    /// Deterministic plant: no oscillation, no pump error.
    fn quiet() -> SimConfig {
        SimConfig {
            variability: false,
            pump_error_cv: 0.0,
            ..SimConfig::default()
        }
    }

    /// The estimator on its own model, B08 noise, 60 g unannounced at 5 h:
    /// RMS error of the estimate and of the raw reading over 16 seeds, and
    /// the quiet mode's mean weight after the meal.
    fn estimate_vs_reading(meal_g: f64) -> (f64, f64, f64) {
        let p = w10_auto().model();
        let (rest, basal) = (REST_MMOL_L, p.bir);
        let period = CONTROL_PERIOD_MIN;
        let steps = (period / ROLLOUT_STEP_MIN) as usize;
        let step = |b, carbs| propagate_period(&p, b, basal, 0.0, carbs, steps);
        let (mut est, mut read, mut quiet, mut n, mut n_after) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for seed in 1..=16 {
            let mut truth = HovorkaState::basal(&p, rest);
            let mut e = Estimator::new(&p, truth);
            let mut sensor = CgmSensor::new(SensorModel::Breton2008, seed);
            for k in 0..64 {
                let g = truth.interstitial_glucose(&p);
                let r = sensor.read(MmolL(g)).to_mmol_l().0;
                e.update(&p, r);
                if k >= 4 {
                    est += (e.state.interstitial_glucose(&p) - g).powi(2);
                    read += (r - g).powi(2);
                    n += 1.0;
                }
                if k > 22 && k < 40 {
                    quiet += e.mode_probabilities()[0];
                    n_after += 1.0;
                }
                truth = step(truth, if k == 20 { meal_g } else { 0.0 });
                e.predict(period, |b| step(b, 0.0));
            }
        }
        ((est / n).sqrt(), (read / n).sqrt(), quiet / n_after)
    }

    #[test]
    fn estimator_beats_the_raw_reading() {
        let (est, read, quiet) = estimate_vs_reading(0.0);
        assert!(
            est < 0.7 * read,
            "no meal: estimate {est:.3} vs reading {read:.3}"
        );
        assert!(quiet > 0.5, "no meal: quiet mode at {quiet:.2}");
        let (est, read, quiet) = estimate_vs_reading(60.0);
        assert!(est < read, "meal: estimate {est:.3} vs reading {read:.3}");
        assert!(quiet < 0.5, "meal: quiet mode still at {quiet:.2}");
    }

    #[test]
    fn open_loop_basal_holds_the_admit_glucose() {
        let cfg = SimConfig {
            use_controller: false,
            admit_glucose_mg_per_dl: 140.0,
            ..quiet()
        };
        let trace = simulate(&cfg);
        for c in &trace.interstitial_mmol_per_l {
            assert!((c - 140.0 / 18.0156).abs() < 1e-9, "{c}");
        }
    }

    #[test]
    fn closed_loop_without_meals_stays_in_range() {
        let trace = simulate(&SimConfig::default());
        let mean = mean_mmol(&trace.interstitial_mmol_per_l);
        assert!((4.4..=8.0).contains(&mean), "mean {mean}");
        assert_eq!(tbr_pct(&trace.interstitial_mmol_per_l), 0.0);
    }

    #[test]
    fn unannounced_meal_lifts_glucose() {
        let cfg = SimConfig {
            duration_hours: 8.0,
            meals: vec![meal(60.0, 60.0, false)],
            ..SimConfig::default()
        };
        let trace = simulate(&cfg);
        let before = mean_mmol(&trace.interstitial_mmol_per_l[..4]);
        let after = mean_mmol(&trace.interstitial_mmol_per_l[6..14]);
        assert!(after > before + 1.0, "before {before:.2}, after {after:.2}");
    }

    #[test]
    fn announcing_a_meal_raises_delivery_sooner() {
        let run = |announced| {
            simulate(&SimConfig {
                duration_hours: 2.0,
                meals: vec![meal(60.0, 60.0, announced)],
                ..quiet()
            })
        };
        let (known, unknown) = (run(true), run(false));
        // The period after the meal: the belief already holds its carbs.
        assert!(known.delivered_u_per_h[5] > unknown.delivered_u_per_h[5]);
    }

    #[test]
    fn closed_loop_corrects_hyperglycemia_down() {
        // The subject rests at 180 mg/dL on its basal; the loop aims lower.
        let cfg = SimConfig {
            admit_glucose_mg_per_dl: 180.0,
            ..SimConfig::default()
        };
        let trace = simulate(&cfg);
        let end = mean_mmol(&trace.interstitial_mmol_per_l[trace.t_min.len() - 8..]);
        assert!(end < 8.0, "still at {end:.2} after 10 h");
    }

    #[test]
    fn bolus_reaches_the_body_at_pump_speed() {
        let cfg = SimConfig {
            duration_hours: 1.0,
            use_controller: false,
            boluses: vec![Bolus {
                start_min: 20.0,
                units: 5.0,
            }],
            ..quiet()
        };
        let trace = simulate(&cfg);
        let extra: Vec<f64> = trace
            .delivered_u_per_h
            .iter()
            .map(|d| (d - trace.resting_basal_u_per_h) / 4.0)
            .collect();
        // All 5 U in the 15-30 min period, none before or after.
        assert!(extra[0].abs() < 1e-12 && extra[2].abs() < 1e-12);
        assert!((extra[1] - 5.0).abs() < 1e-12, "{extra:?}");
    }

    /// True glucose after a 60 g meal with its full ICR bolus.
    /// The minimum falls 4.8-5.5 h in on all 16 seeds.
    const HOURS_AFTER_MEAL: f64 = 8.0;

    fn bolused_meal(sensor: SensorModel, sensor_seed: u64) -> Vec<f64> {
        let mut cfg = SimConfig {
            duration_hours: HOURS_AFTER_MEAL,
            meals: vec![meal(60.0, 60.0, true)],
            sensor,
            sensor_seed,
            ..SimConfig::default()
        };
        cfg.boluses = icr_boluses(&cfg.subject, &cfg.meals, 1.0);
        simulate(&cfg).interstitial_mmol_per_l
    }

    #[test]
    fn announced_meal_with_icr_bolus_stays_above_hypo() {
        let ig = bolused_meal(SensorModel::Exact, 1);
        assert_eq!(tbr_pct(&ig), 0.0, "noise-free meal went below 70 mg/dL");
        let max = ig.iter().copied().fold(0.0, f64::max);
        assert!(max <= 14.0, "announced meal spiked to {max:.2}");
        for seed in 1..=4 {
            let ig = bolused_meal(SensorModel::Breton2008, seed);
            assert_eq!(tbr_pct(&ig), 0.0, "seed {seed}: below 70 mg/dL");
        }
    }

    #[test]
    fn closed_loop_beats_open_loop_on_unannounced_meals() {
        let mut cfg = SimConfig {
            meals: vec![meal(300.0, 60.0, false), meal(700.0, 60.0, false)],
            ..SimConfig::default()
        };
        let closed = tir_pct(&simulate(&cfg).interstitial_mmol_per_l);
        cfg.use_controller = false;
        let open = tir_pct(&simulate(&cfg).interstitial_mmol_per_l);
        assert!(closed > open, "closed {closed:.1}% vs open {open:.1}%");
    }

    /// A mean over 8 seed sets: W10's variability makes single days swing
    /// (worst of 24: 8% TBR).
    #[test]
    fn realistic_day_stays_in_range() {
        let (mut tir, mut low) = (0.0, 0.0);
        for seed in 1..=8 {
            let ig = simulate(&realistic_day(seed)).interstitial_mmol_per_l;
            tir += tir_pct(&ig) / 8.0;
            low += tbr_pct(&ig) / 8.0;
        }
        assert!(tir >= 70.0, "TIR {tir:.1}%");
        assert!(low <= 4.0, "TBR {low:.1}%");
    }

    #[test]
    fn hypo_admit_suspends_the_pump() {
        let cfg = SimConfig {
            admit_glucose_mg_per_dl: 60.0,
            duration_hours: 1.0,
            ..SimConfig::default()
        };
        let trace = simulate(&cfg);
        assert!(trace.reading_mmol_per_l[0] < tir_tuner_aps::controller::HYPO_CUTOFF_MMOL_L);
        assert_eq!(trace.delivered_u_per_h[0], 0.0);
    }

    #[test]
    fn deterministic_from_seeds() {
        let mut cfg = SimConfig {
            meals: vec![meal(60.0, 40.0, true)],
            ..SimConfig::default()
        };
        let (a, b) = (simulate(&cfg), simulate(&cfg));
        assert_eq!(a.reading_mmol_per_l, b.reading_mmol_per_l);
        assert_eq!(a.delivered_u_per_h, b.delivered_u_per_h);
        cfg.variability_seed = 4;
        assert_ne!(
            simulate(&cfg).interstitial_mmol_per_l,
            a.interstitial_mmol_per_l
        );
    }
}
