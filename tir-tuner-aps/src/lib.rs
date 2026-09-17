//! CamAPS FX style hybrid closed-loop controller core.
//!
//! This crate implements a minimal, deterministic version of the
//! Cambridge hybrid closed-loop algorithm building blocks:
//!
//! * [`hovorka`] - the 10-dimensional glucoregulatory state model,
//!   subcutaneous insulin / gut absorption kinetics, EGP and
//!   interstitial glucose dynamics.
//! * [`controller`] - the NMPC style dosing calculator for the insulin
//!   pump including the hard hypoglycemia cutoff and maximum delivery
//!   limits.
//! * [`imm`] - the interacting multiple model mode-probability bookkeeping.
//!
//! The Kani formal verification suite lives in [`verification`] and is
//! only compiled when `cfg(kani)` is set (i.e. under `cargo kani`).

pub mod controller;
pub mod hovorka;
pub mod imm;

#[cfg(kani)]
pub mod verification;

/// Default glucose target (mmol/L). Ware et al. 2022.
pub const TARGET_GLUCOSE_MMOL_L: f64 = 5.8;
/// Lower end of the user-adjustable target range (mmol/L).
pub const TARGET_RANGE_MIN_MMOL_L: f64 = 4.4;
/// Upper end of the user-adjustable target range (mmol/L).
pub const TARGET_RANGE_MAX_MMOL_L: f64 = 11.0;
/// Hard hypoglycemia cutoff: mandatory zero delivery below this
/// interstitial glucose reading (mmol/L).
pub const HARD_HYPO_CUTOFF_MMOL_L: f64 = 4.4;
/// Ease-off (exercise) target (mmol/L).
pub const EASE_OFF_TARGET_MMOL_L: f64 = 7.0;
/// Boost mode insulin intensification factor (Ware et al. 2022).
pub const BOOST_DELIVERY_FACTOR: f64 = 1.35;
/// Proportional dose gain used by the mode-aware controller (U/h per
/// mmol/L above target).
///
/// Illustrative internal constant: a closed-loop proportional stand-in
/// for the one-step grid NMPC, not a published CamAPS parameter (the
/// papers specify the NMPC cost, section 5.1, not a fixed gain).
pub const DOSE_GAIN_U_H_PER_MMOL_L: f64 = 0.5;

/// Time in range band: lower bound (mmol/L).
pub const TIME_IN_RANGE_MIN_MMOL_L: f64 = 3.9;
/// Time in range band: upper bound (mmol/L).
pub const TIME_IN_RANGE_MAX_MMOL_L: f64 = 10.0;
/// Sensor measurement noise standard deviation (mmol/L) used in
/// in-silico scenarios.
pub const SENSOR_NOISE_SD_MMOL_L: f64 = 0.5;

/// Shared configuration for the native `proptest` property suite.
#[cfg(test)]
pub(crate) mod test_support {
    use proptest::prelude::ProptestConfig;

    /// Property-test configuration. `TIR_TUNER_SOAK_ITERS` raises the case
    /// count for the opt-in soak run; the default keeps `cargo test`
    /// well under the fast budget.
    pub fn config() -> ProptestConfig {
        let mut config = ProptestConfig::default();
        if let Ok(value) = std::env::var("TIR_TUNER_SOAK_ITERS") {
            if let Ok(cases) = value.parse::<u32>() {
                config.cases = cases;
            }
        }
        config
    }
}
