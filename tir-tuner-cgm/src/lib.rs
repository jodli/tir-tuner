#![warn(missing_docs)]
//! CGM sensor model: interstitial glucose to a noise-corrupted reading.
//!
//! The sensor reads the body's interstitial glucose and returns a value
//! corrupted by a first-order autoregressive measurement error, the
//! structure identified by Facchinetti et al. 2014 `[F14]` and Breton &
//! Kovatchev 2008 `[B08]`: a lagged, auto-correlated error on top of the true
//! interstitial value. Calibration gain sits here, not on the body,
//! which keeps the observation model genuinely separate from the
//! physiology.
//!
//! The error follows `e_{t+1} = alpha1 * e_t + w_t` with `w_t` zero-mean
//! Gaussian of standard deviation `noise_sd` (default
//! [`SENSOR_NOISE_SD_MMOL_L`]); the reading is
//! `calibration_gain * interstitial + e` clipped at zero, because a
//! closed-loop controller must never observe a negative glucose.
//!
//! As with the aps and body crates, the sensor is deterministic from a
//! seed so the sim engine reproduces a scenario end-to-end.
//!
//! For a patient-facing companion page, see `docs/patient/sensor.md`.

/// Default sensor measurement noise standard deviation (mmol/L) used in
/// in-silico scenarios.
pub const SENSOR_NOISE_SD_MMOL_L: f64 = 0.5;

pub mod device;

pub use device::{CgmSensor, SensorParams, SensorReading};

#[cfg(kani)]
pub mod verification;