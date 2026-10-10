//! Kani proofs for the sensor (only built under `cargo kani`). The Johnson
//! transform's `sinh` is out of CBMC's reach; its statistics are native
//! tests in `device.rs`.

use super::device::{ar_step, reading};

/// For innovations within ±5 SD, the eq 7 driver never leaves [-12, 12]:
/// 0.7 (12 + 5) = 11.9.
#[kani::proof]
pub fn verify_ar_driver_stays_bounded() {
    let e: f64 = kani::any();
    let v: f64 = kani::any();
    kani::assume(e >= -12.0 && e <= 12.0);
    kani::assume(v >= -5.0 && v <= 5.0);
    let next = ar_step(Some(e), v);
    kani::assert(next >= -12.0 && next <= 12.0, "driver stays in [-12, 12]");
}

/// A reading is never negative and is a whole number.
#[kani::proof]
pub fn verify_reading_is_whole_and_non_negative() {
    let mg_dl: f64 = kani::any();
    kani::assume(mg_dl >= -500.0 && mg_dl <= 1000.0);
    let r = reading(mg_dl).0;
    kani::assert(r >= 0.0, "never negative");
    kani::assert(r == r.trunc(), "whole mg/dL");
}
