#![warn(missing_docs)]
//! CGM sensor model: interstitial glucose to a whole-mg/dL reading.
//!
//! The error is Breton & Kovatchev 2008 eq 7: an AR(1) driver per 15-min
//! sample, mapped through a Johnson SU transform to a non-Gaussian error in
//! mg/dL. B08 fitted it on the FreeStyle Navigator. No source in the folder
//! models Libre 3 error, so this is the closest published one.
//!
//! For a patient-facing companion page, see `docs/patient/sensor.md`.

pub mod device;

pub use device::{CgmSensor, SAMPLE_INTERVAL_MIN, SensorModel};

#[cfg(kani)]
pub mod verification;
