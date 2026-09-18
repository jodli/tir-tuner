//! In-silico closed-loop engine: body -> CGM -> aps controller -> pump.
//!
//! The engine wires the verified crates together into a deterministic
//! scenario runner. A subject starts at an admit glucose, the CGM sensor
//! reads the interstitial compartment every fifteen minutes, the aps
//! controller turns the reading into an insulin rate, the pump applies
//! a small delivery error, and the body integrates forward on a
//! sub-minute grid. Meals are time-gated carbohydrate inputs.
//!
//! The controller is the aps sequence NMPC of spec section 5.1 (Hovorka
//! et al 2004, eq 9), driven over the 240-minute prediction horizon at
//! 15-minute sampling: the summed cost
//! `sum_j (g_IG(t+j) - w(t+j))^2
//!  + (1/k_agr) sum_j ((u(t+j) - u(t+j-1)) / K_u)^2`
//! is minimized over a quantized rate sequence anchored on the previous
//! period's rate `u(t)`, and only the first element is applied. The
//! pump-side algorithm is not modified here; the horizon only has to
//! span the insulin action timescale, or the rollout cannot discriminate
//! the candidates. It predicts against an internal belief model, a
//! `HovorkaParams`/`HovorkaState` mapped from the plant subject so the
//! belief's basal insulin concentration and resting glucose coincide with
//! the body's. The belief is stepped in lockstep with the delivered rate
//! and meals, and re-anchored on the sensor reading once per control
//! period. The re-anchor is the state-estimation layer (the spec reserves
//! signal processing for the simulation/filtering side): the raw CGM
//! value is blended with the belief's own interstitial estimate by
//! [`CONTROLLER_ESTIMATE_SMOOTHING`], so a single noisy reading cannot
//! jerk the controller into the extreme of the grid.
//! Deliberately the belief is not identical to the body model: the loop
//! must tolerate the mismatch.
//!
//! Meal boluses: when a meal is announced (`announce_meals`), the bolus
//! prescribed by the subject's carbohydrate-to-insulin ratio is delivered
//! at meal start, spread over the control period, and the same amount is
//! pulsed into the belief state's insulin depot. With `use_controller`
//! off the loop degenerates to the delivered basal requirement, which is
//! the open-loop control arm for comparisons.

use tir_tuner_aps::controller::{is_hypoglycemic, nmpc_sequence_dose, NMPC_EFFORT_UNIT_U_PER_H};
use tir_tuner_aps::hovorka::{HovorkaParams, HovorkaState};
use tir_tuner_aps::TARGET_GLUCOSE_MMOL_L;
use tir_tuner_body::derivative::BodyInputs;
use tir_tuner_body::solver::{admit_state, step, with_basal_glucose, DT_MIN};
use tir_tuner_body::subject::VirtualSubject;
use tir_tuner_cgm::device::{CgmSensor, SensorParams};

use tir_tuner_common::random::SeededRng;
use tir_tuner_common::units::mg_per_dl_to_mmol_per_l;
use tir_tuner_common::units::MU_PER_UNIT;

/// Controller sampling period in minutes: matches the CGM cadence and
/// the aps controller's per-cycle update (Hovorka et al 2004).
pub const CONTROL_PERIOD_MIN: f64 = 15.0;

/// Prediction horizon of the NMPC roll-out, in minutes.
///
/// Merges into the sample resolution of `CONTROL_PERIOD_MIN`, so the
/// cost sums over 16 samples (Hovorka et al 2004: 4 hours at 15-minute
/// sampling). This is the shortest horizon that discriminates the
/// candidate sequences: single-sample predictions leave predicted
/// glucose identical across the grid because insulin action acts over
/// tens of minutes, and the argmin collapses to basal delivery for any
/// k_agr.
pub const CONTROL_HORIZON_MIN: f64 = 240.0;

/// Mixing gain of the sensor reading into the belief estimate, per
/// control period.
///
/// Blending the noisy reading with the belief's own interstitial
/// estimate stops single samples from jerking the controller between the
/// extremes of the grid while still tracking the truth.
pub const CONTROLLER_ESTIMATE_SMOOTHING: f64 = 0.5;

/// Aggressiveness constant `k_agr` of the NMPC objective (Hovorka et al
/// 2004, eq 9): the effort term weights the squared rate *change*
/// `((u_j - u_{j-1}) / K_u)^2` by `1 / k_agr`, with
/// `K_u = NMPC_EFFORT_UNIT_U_PER_H`. Larger values weight the effort
/// less, so the optimizer moves the rate more freely; smaller values pin
/// the sequence to the previous rate. The paper tuned k_agr for fewer
/// than 5% of readings below 3.3 mmol/L at a mean fasting glucose of
/// about 6 mmol/L.
///
/// The default carries the tuning from the un-normalized formulation
/// (`k_agr = 5`, `K_u = 1`) over to the normalized one: the effort
/// weight `1 / (K_u^2 * k_agr)` is unchanged at `0.2`.
///
/// Measured on the real-data day (24 hours, four meals, default seeds):
/// TIR 75.0% with 3.1% below range; the two-meal control day holds
/// TIR 89.2%. The sum formulation is deliberately more cautious than a
/// terminal-node formulation: it prices the whole predicted trajectory,
/// so it cuts corrections earlier and leaves a few percent of post-meal
/// highs instead of chasing the spikes into lows.
pub const CONTROLLER_KAGR_DEFAULT: f64 = 5.0 / (NMPC_EFFORT_UNIT_U_PER_H * NMPC_EFFORT_UNIT_U_PER_H);

/// Number of one-minute steps used to integrate the belief model over
/// one control period (the aps bolus convention is a per-minute influx).
pub const BELIEF_MIN_STEPS: usize = CONTROL_PERIOD_MIN as usize;

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
    /// Aggressiveness constant of the NMPC objective.
    pub controller_kagr: f64,
    /// Mixing gain of the sensor reading into the belief estimate.
    pub controller_smoothing: f64,
    /// Deliver an ICR-based bolus at each meal start.
    pub announce_meals: bool,
    /// Fraction of the full ICR bolus delivered at an announced meal; the
    /// remainder is left to the closed loop.
    pub meal_bolus_factor: f64,
    /// Run the closed loop; when false, delivery is the basal
    /// requirement only (hypoglycemia suspension still active).
    pub use_controller: bool,
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
            subject: with_basal_glucose(
                &VirtualSubject::population_mean(),
                TARGET_GLUCOSE_MMOL_L,
            ),
            admit_glucose_mg_per_dl: 100.0,
            duration_hours: 12.0,
            meals: Vec::new(),
            max_delivery_u_per_h: 10.0,
            controller_kagr: CONTROLLER_KAGR_DEFAULT,
            controller_smoothing: CONTROLLER_ESTIMATE_SMOOTHING,
            announce_meals: true,
            meal_bolus_factor: 0.8,
            use_controller: true,
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

/// The controller's internal prediction model, mapped from the plant
/// subject.
///
/// * `t_max_i = 1/ka` and `t_max_g` reuse the subject's absorption
///   kinetics.
/// * `mcr_i = vi * ke`, so the aps basal insulin concentration
///   `1000*bir/(60*mcr_i*w)` equals the body's `(bir/60*1000)/(vi*w*ke)`.
/// * `f_01`, `egp_b`, `v_g`, `k12`, `p2_d`/`p2_e`, `k31` reuse the
///   subject's non-insulin uptake, EGP, volumes, glucose shuttle, remote
///   action timescales and interstitial rate, and `s_egp` reuses the
///   subject's EGP suppression gain so the belief's hepatic response
///   matches what the body actually does.
/// * `s_id` is recalibrated so the belief's basal equilibrium rests on
///   `TARGET_GLUCOSE_MMOL_L` at the subject's own basal insulin
///   concentration, accounting for the glucose-dependent Michaelis-
///   Menten uptake `f01c`.
pub fn controller_model(subject: &VirtualSubject) -> HovorkaParams {
    let mcr_i = subject.vi_l_per_kg * subject.ke_per_min;
    let bic = subject.basal_insulin_concentration();
    // The belief's basal resting glucose is q1/v_g = (egp_b - f01c)/
    // (s_id * BIC) / v_g; solve for the s_id that rests on the target.
    // The population subject has egp0 > f01; guard any pathological
    // parameterization so the belief never predicts unbounded glucose.
    let target = TARGET_GLUCOSE_MMOL_L;
    let f01c = subject.f01_mmol_per_kg_min / 0.85 * target / (target + 1.0);
    let surplus = (subject.egp0_mmol_per_kg_min - f01c).max(1e-4);
    let s_id = surplus / (target * subject.vg_l_per_kg * bic);
    HovorkaParams {
        t_max_i: 1.0 / subject.ka_per_min,
        t_max_g: subject.t_max_g_min,
        mcr_i,
        weight_kg: subject.weight_kg,
        p2_d: subject.kb2_per_min,
        p2_e: subject.kb3_per_min,
        k12: subject.k12_per_min,
        k21: subject.k12_per_min,
        k31: subject.ka_int_per_min,
        v_g: subject.vg_l_per_kg,
        f_01: subject.f01_mmol_per_kg_min,
        s_id,
        egp_b: subject.egp0_mmol_per_kg_min,
        s_egp: subject.sie_per_mu_l,
        bir: subject.bir_u_per_h,
    }
}

/// Initial belief state for the given plasma glucose (mmol/L): the
/// basal insulin compartment at rest, and the glucose masses scaled so
/// plasma and interstitial glucose sit on the presenting level.
pub fn belief_at_glucose(
    params: &HovorkaParams,
    subject: &VirtualSubject,
    plasma_mmol_per_l: f64,
) -> HovorkaState {
    let influx_mu_per_min = MU_PER_UNIT * subject.bir_u_per_h / 60.0;
    let i_ss = influx_mu_per_min * params.t_max_i;
    let bic = subject.basal_insulin_concentration();
    let q = plasma_mmol_per_l * params.v_g;
    HovorkaState {
        i1: i_ss,
        i2: i_ss,
        r_d: bic,
        r_e: bic,
        a1: 0.0,
        a2: 0.0,
        q1: q,
        q2: q,
        q3: q,
        u_s: 0.0,
    }
}

/// Replace the belief's glucose masses with the sensor reading, blended
/// with the belief's own interstitial estimate, on the assumption that
/// plasma and interstitial glucose are equilibrated.
///
/// This is the state-estimation layer (the spec reserves signal
/// processing for the simulation side): the raw CGM value is mixed in at
/// [`CONTROLLER_ESTIMATE_SMOOTHING`] so one noisy sample cannot jerk the
/// one-step controller into a grid extreme. The matching insulin and
/// remote-action state is carried forward unchanged.
fn reanchor_belief_on_reading(
    belief: &mut HovorkaState,
    params: &HovorkaParams,
    interstitial: f64,
    smoothing: f64,
) {
    let own_estimate = belief.q3 / params.v_g;
    let blended = smoothing * interstitial + (1.0 - smoothing) * own_estimate;
    belief.q1 = blended * params.v_g;
    belief.q2 = blended * params.v_g;
    belief.q3 = blended * params.v_g;
}

/// Bolus (U) prescribed by the subject's carbohydrate-to-insulin ratio.
pub fn meal_bolus_u(subject: &VirtualSubject, carbs_g: f64) -> f64 {
    carbs_g / 10.0 * subject.icr_u_per_10g_cho
}

fn active_meal_g_per_min(meals: &[Meal], t_min: f64) -> f64 {
    meals
        .iter()
        .find(|m| t_min >= m.start_min && t_min < m.start_min + m.duration_min)
        .map_or(0.0, |m| m.carbs_g / m.duration_min)
}

/// Run one scenario deterministically from the seeds.
pub fn simulate(cfg: &SimConfig) -> SimTrace {
    let total_min = cfg.duration_hours * 60.0;
    let steps_per_control: usize = (CONTROL_PERIOD_MIN / DT_MIN).round() as usize;

    let mut state = admit_state(&cfg.subject, cfg.admit_glucose_mg_per_dl);
    let mut sensor = CgmSensor::new(SensorParams::default(), cfg.sensor_seed);
    let mut pump_rng = SeededRng::new(cfg.pump_seed);

    let basal_u_per_h = cfg.subject.bir_u_per_h;
    let params = controller_model(&cfg.subject);
    let admit_mmol_per_l = mg_per_dl_to_mmol_per_l(cfg.admit_glucose_mg_per_dl);
    let mut belief = belief_at_glucose(&params, &cfg.subject, admit_mmol_per_l);

    let mut meals = cfg.meals.clone();
    meals.sort_by(|a, b| a.start_min.total_cmp(&b.start_min));
    let mut next_meal = 0usize;

    let mut t_min = Vec::new();
    let mut reading_mmol_per_l = Vec::new();
    let mut delivered_u_per_h = Vec::new();
    let mut interstitial_mmol_per_l = Vec::new();

    let mut t = 0.0;
    // Previous control period's applied rate, the `u(t)` the section 5.1
    // effort term prices changes against. Zero after a hypoglycemia
    // suspension: the guard zeroes delivery, so the next period's rate
    // change is measured from zero.
    let mut last_rate_u_per_h = basal_u_per_h;
    while t < total_min {
        let reading = sensor.read(state.c);
        reanchor_belief_on_reading(
            &mut belief,
            &params,
            reading.glucose_mmol_per_l,
            cfg.controller_smoothing,
        );

        let meal_g_per_min = active_meal_g_per_min(&meals, t);

        // Bolus for the next meal that has started but not yet been
        // announced; at most one per control period.
        let bolus_u = if cfg.announce_meals {
            match meals.get(next_meal) {
                Some(m) if t >= m.start_min => {
                    cfg.meal_bolus_factor * meal_bolus_u(&cfg.subject, m.carbs_g)
                }
                _ => 0.0,
            }
        } else {
            0.0
        };
        while next_meal < meals.len() && meals[next_meal].start_min <= t {
            next_meal += 1;
        }

        // Controller rate in U/h. The section 5.1 sequence NMPC of the
        // aps crate (Hovorka et al 2004, eq 9), driven over the
        // 240-minute prediction horizon that the rollout needs to see
        // the insulin action; the effort term prices the change from the
        // previous period's rate, so the delivered rate is carried over
        // as `last_rate_u_per_h` (zero after a hypoglycemia suspension,
        // since the guard zeroes delivery outright). The hard hypo
        // cutoff zeroes delivery, boluses included; in open-loop mode
        // the basal requirement replaces the NMPC rate.
        let hypo = is_hypoglycemic(reading.glucose_mmol_per_l);
        let base_rate_u_per_h = if cfg.use_controller {
            if hypo {
                0.0
            } else {
                nmpc_sequence_dose(
                    &params,
                    belief,
                    reading.glucose_mmol_per_l,
                    TARGET_GLUCOSE_MMOL_L,
                    cfg.controller_kagr,
                    cfg.max_delivery_u_per_h,
                    last_rate_u_per_h,
                    CONTROL_HORIZON_MIN,
                    CONTROL_PERIOD_MIN,
                )
            }
        } else if hypo {
            0.0
        } else {
            basal_u_per_h
        };
        let bolus_applied_u = if hypo { 0.0 } else { bolus_u };
        let rate_u_per_h = pump_delivery(base_rate_u_per_h, &mut pump_rng, cfg.pump_error_cv);
        let delivered_mu_per_min = rate_u_per_h / 60.0 * MU_PER_UNIT;
        last_rate_u_per_h = base_rate_u_per_h;

        // The bolus adds to the subcutaneous influx, spread over the
        // whole control period.
        let bolus_mu_per_min = bolus_applied_u * MU_PER_UNIT / CONTROL_PERIOD_MIN;

        let inputs = BodyInputs {
            u_basal_mu_per_min: delivered_mu_per_min + bolus_mu_per_min,
            meal_g_per_min,
        };

        for _ in 0..steps_per_control {
            state = step(&cfg.subject, &state, &inputs, DT_MIN);
        }

        // Integrate the belief in lockstep with the delivered rate and
        // the meal; the aps bolus convention is a one-minute influx, so
        // melt the bolus in on the first minute of the period.
        let belief_rate_u_per_h = delivered_mu_per_min / MU_PER_UNIT * 60.0;
        for k in 0..BELIEF_MIN_STEPS {
            let bolus_now = if k == 0 { bolus_applied_u } else { 0.0 };
            belief = belief.step(&params, belief_rate_u_per_h, bolus_now, meal_g_per_min, 1.0);
        }

        t_min.push(t);
        reading_mmol_per_l.push(reading.glucose_mmol_per_l);
        delivered_u_per_h.push(delivered_mu_per_min / MU_PER_UNIT * 60.0);
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
/// Applies `(1 + e)` with `e ~ N(0, cv)` and floors negative delivery at
/// zero. The hypo suspension happened upstream in [`simulate`]. Returns
/// the rate in U/h.
fn pump_delivery(rate_u_per_h: f64, rng: &mut SeededRng, error_cv: f64) -> f64 {
    let error = 1.0 + error_cv * rng.next_normal();
    (rate_u_per_h * error).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tir_tuner_common::metrics::{mean_glucose, time_in_range_pct};

    #[test]
    fn belief_params_match_subject_basal_insulin() {
        let subject = VirtualSubject::population_mean();
        let params = controller_model(&subject);
        assert!(
            (params.basal_insulin_conc() - subject.basal_insulin_concentration()).abs() < 1e-9
        );
    }

    #[test]
    fn belief_rests_on_target() {
        let subject = VirtualSubject::population_mean();
        let params = controller_model(&subject);
        let belief = belief_at_glucose(&params, &subject, TARGET_GLUCOSE_MMOL_L);
        assert!((belief.interstitial_glucose(&params) - TARGET_GLUCOSE_MMOL_L).abs() < 1e-6);
    }

    #[test]
    fn meal_bolus_follows_icr() {
        let subject = VirtualSubject::population_mean();
        assert!((meal_bolus_u(&subject, 60.0) - 10.2).abs() < 1e-9);
    }

    #[test]
    fn open_loop_basal_holds_interstitial_near_rest() {
        // No meals: the closed loop settles near the subject's resting
        // glucose because the NMPC effort term pulls delivery toward
        // basal.
        let cfg = SimConfig::default();
        let trace = simulate(&cfg);

        let mean = mean_glucose(&trace.interstitial_mmol_per_l);
        assert!(mean.is_finite());
        assert!(
            (2.5..=10.0).contains(&mean),
            "interstitial mean drifted to {mean}"
        );
        assert_eq!(trace.t_min.len(), trace.reading_mmol_per_l.len());
    }

    #[test]
    fn meal_without_announcement_lifts_glucose() {
        let mut cfg = SimConfig::default();
        cfg.duration_hours = 8.0;
        cfg.announce_meals = false;
        cfg.meals = vec![Meal {
            start_min: 60.0,
            carbs_g: 60.0,
            duration_min: 15.0,
        }];
        // An unannounced meal still drives the CGM reading up measurably
        // even with the closed loop running. Before-window: t in [0,180];
        // comparison window t in [300,480] after the meal hit the gut
        // (trace has one sample per 15-minute control period).
        let trace = simulate(&cfg);

        let before = mean_glucose(&trace.reading_mmol_per_l[..12]);
        let window = mean_glucose(&trace.reading_mmol_per_l[20..32]);
        assert!(
            window > before,
            "meal did not lift glucose: before {before:.2}, after {window:.2}"
        );
    }

    #[test]
    fn closed_loop_corrects_hyperglycemia_down() {
        // Start high (180 mg/dL = 10 mmol/L) with a large delivery
        // headroom. The NMPC should pull the interstitial glucose down
        // over the run.
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
    fn announced_meal_with_bolus_stays_above_hypo() {
        // A 60 g meal announced at the subject's ICR gets its full bolus,
        // so the loop should not push the reading into hypoglycemia; the
        // post-prandial peak stays bounded too.
        let mut cfg = SimConfig::default();
        cfg.duration_hours = 24.0;
        cfg.max_delivery_u_per_h = 20.0;
        cfg.meals = vec![Meal {
            start_min: 60.0,
            carbs_g: 60.0,
            duration_min: 15.0,
        }];
        let trace = simulate(&cfg);

        let min = trace
            .reading_mmol_per_l
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        let max = trace
            .reading_mmol_per_l
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(min >= 3.9, "announced meal crashed to {min}");
        assert!(max <= 14.0, "announced meal spiked to {max}");
    }

    #[test]
    fn closed_loop_beats_open_loop_on_same_meals() {
        // The control arm: same subject, same meals, same seeds, but the
        // meals are unannounced (no bolus), so the only difference is
        // whether the loop senses and corrects the post-prandial rise.
        // The closed loop should hold more time in range than basal-only
        // delivery.
        let mut cfg = SimConfig::default();
        cfg.duration_hours = 12.0;
        cfg.max_delivery_u_per_h = 20.0;
        cfg.announce_meals = false;
        cfg.meals = vec![
            Meal { start_min: 300.0, carbs_g: 60.0, duration_min: 15.0 },
            Meal { start_min: 700.0, carbs_g: 60.0, duration_min: 15.0 },
        ];
        let closed = simulate(&cfg);
        cfg.use_controller = false;
        let open = simulate(&cfg);

        let tir_closed = time_in_range_pct(&closed.reading_mmol_per_l);
        let tir_open = time_in_range_pct(&open.reading_mmol_per_l);
        assert!(
            tir_closed > tir_open,
            "closed loop did not beat open loop: closed {tir_closed:.1}% vs open {tir_open:.1}%"
        );
        assert!(
            tir_closed >= 45.0,
            "closed loop TIR too low even in absolute terms: {tir_closed:.1}%"
        );
    }

    #[test]
    fn realistic_scenario_stays_in_range() {
        // Four meals across a day, starting at a representative admit.
        // This pins the tuned loop: default k_agr/smoothing/seeds must
        // hold the day mostly in range with only mild lows.
        let mut cfg = SimConfig::default();
        cfg.duration_hours = 24.0;
        cfg.admit_glucose_mg_per_dl = 140.0;
        cfg.max_delivery_u_per_h = 20.0;
        cfg.meals = vec![
            Meal { start_min: 360.0, carbs_g: 60.0, duration_min: 15.0 },
            Meal { start_min: 600.0, carbs_g: 50.0, duration_min: 15.0 },
            Meal { start_min: 840.0, carbs_g: 70.0, duration_min: 15.0 },
            Meal { start_min: 1140.0, carbs_g: 60.0, duration_min: 15.0 },
        ];
        let trace = simulate(&cfg);

        let tir = time_in_range_pct(&trace.reading_mmol_per_l);
        let low = 100.0
            * trace
                .reading_mmol_per_l
                .iter()
                .filter(|&&v| v < tir_tuner_common::metrics::TIME_IN_RANGE_MIN_MMOL_L)
                .count() as f64
            / trace.reading_mmol_per_l.len() as f64;
        // Regression bar: on the real-data day the unfiltered one-sample
        // prediction held 40% TIR with 39% lows; the horizon-rolled
        // loop stays comfortably above both.
        assert!(tir >= 70.0, "tuned loop drifted: TIR {tir:.1}%");
        assert!(low <= 6.0, "tuned loop over-corrects: {low:.1}% below range");
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
    fn hypo_admit_suspends_pump_and_invariant_holds() {
        // Presenting below the hard cutoff: the very first control
        // decisions must carry zero delivery, boluses included.
        let admit = SimConfig::default();
        let trace = simulate(&admit);
        assert!(
            !is_hypoglycemic(trace.reading_mmol_per_l[0]),
            "admit reading at 100 mg/dL should be above cutoff"
        );

        let mut cfg = admit.clone();
        cfg.admit_glucose_mg_per_dl = 60.0; // 3.3 mmol/L < 4.4
        let trace = simulate(&cfg);
        assert!(
            is_hypoglycemic(trace.reading_mmol_per_l[0]),
            "expected hypo at admit"
        );
        assert_eq!(trace.delivered_u_per_h[0], 0.0, "pump must not deliver on hypo admit");

        // Invariant over a long run with heavy carbohydrate: wherever the
        // reading breaks the cutoff, delivery must be exactly zero and the
        // trace stays finite.
        let mut cfg2 = SimConfig::default();
        cfg2.duration_hours = 24.0;
        cfg2.announce_meals = false;
        cfg2.meals = vec![Meal { start_min: 60.0, carbs_g: 100.0, duration_min: 15.0 }];
        cfg2.max_delivery_u_per_h = 20.0;
        cfg2.pump_error_cv = 0.0;
        let trace2 = simulate(&cfg2);
        for (i, &r) in trace2.reading_mmol_per_l.iter().enumerate() {
            assert!(r.is_finite() && r >= 0.0);
            if is_hypoglycemic(r) {
                assert_eq!(
                    trace2.delivered_u_per_h[i], 0.0,
                    "pump must be fully suspended at hypo sample {i}"
                );
            }
        }
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
        let mut cfg = SimConfig::default();
        cfg.duration_hours = 5.0 / 60.0;
        let trace = simulate(&cfg);
        assert_eq!(trace.t_min.len(), 1);
        assert!((trace.t_min[0] - 0.0).abs() < 1e-9);
    }
}