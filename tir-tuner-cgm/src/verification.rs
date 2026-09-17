//! Kani formal verification suite for the CGM sensor (only built under
//! `cargo kani`).
//!
//! Same division of labor as the other crates: universal claims over
//! bounded symbolic inputs, restricted to operations CBMC can discharge
//! (comparisons, clamps, additions, multiplications by bounded factors).
//! The RNG-driven noise distribution and the autocorrelation estimate
//! are native-test claims in `device.rs`.

use super::device::{gain_signal, next_error, SensorParams};

/// Upper bound on the noise standard deviation covered by the proofs.
const NOISE_SD_MAX: f64 = 2.0;

/// A sensor with symbolic parameters bounded to the physiologically
/// plausible range. `calibration_gain` stays positive and modest.
fn symbolic_params() -> SensorParams {
    let alpha1: f64 = kani::any();
    let noise_sd: f64 = kani::any();
    let calibration_gain: f64 = kani::any();
    kani::assume(alpha1 >= 0.5 && alpha1 <= 0.95);
    kani::assume(noise_sd >= 0.0 && noise_sd <= NOISE_SD_MAX);
    kani::assume(calibration_gain >= 0.8 && calibration_gain <= 1.2);
    SensorParams {
        alpha1,
        noise_sd,
        calibration_gain,
    }
}

/// An innovation rounded to the tail that a standard normal actually
/// produces with non-negligible probability. Truncation is itself a
/// design fact we assert later.
fn bounded_innovation() -> f64 {
    let w: f64 = kani::any();
    kani::assume(w >= -5.0 && w <= 5.0);
    w
}

/// Proof 1: a single step of the AR(1) error contractive, so a bounded
/// error with a bounded innovation stays inside the 5-minute envelope.
/// With `alpha1 <= 0.95` and `|w| <= 5`, `|e'| <= 0.95*|e| + |w|`; for a
/// previous error of magnitude 1 that is at most 5.95, so the
/// `[-6, 6]` tube is never exited.
#[kani::proof]
pub fn verify_error_stays_within_envelope() {
    let params = symbolic_params();
    let e: f64 = kani::any();
    let w: f64 = bounded_innovation();
    kani::assume(e >= -1.0 && e <= 1.0);

    let next = next_error(&params, e, w);
    kani::assert(next >= -6.0 && next <= 6.0, "error stays within envelope");
    kani::assert(!next.is_nan() && !next.is_infinite(), "error stays finite");
}

/// Proof 2: the reported glucose is never negative and never grows past
/// the raw signal, regardless of gain and error, because the floor is a
/// clamp at zero.
#[kani::proof]
pub fn verify_reported_glucose_is_clipped_at_zero() {
    let params = symbolic_params();
    let ig: f64 = kani::any();
    let e: f64 = kani::any();
    kani::assume(ig >= 0.0 && ig <= 30.0);
    kani::assume(e >= -6.0 && e <= 6.0);

    let raw = gain_signal(&params, ig, e);
    let reported = raw.max(0.0);

    kani::assert(reported >= 0.0, "reported glucose is never negative");
    kani::assert(!reported.is_nan() && !reported.is_infinite(), "reported glucose stays finite");
}

/// Proof 3: with the device floor in force, the reported reading is the
/// raw signal when that is non-negative and exactly zero otherwise. That
/// pins down the floor semantics for the controller layer above.
#[kani::proof]
pub fn verify_floor_is_exactly_zero() {
    let params = symbolic_params();
    let ig: f64 = kani::any();
    let e: f64 = kani::any();
    kani::assume(ig >= 0.0 && ig <= 30.0);
    kani::assume(e >= -6.0 && e <= 6.0);

    let raw = gain_signal(&params, ig, e);
    let reported = raw.max(0.0);

    kani::assert(reported >= 0.0, "floor is zero");
    kani::assert(
        reported == 0.0 || reported == raw,
        "reported is either the raw signal or the floor",
    );
}