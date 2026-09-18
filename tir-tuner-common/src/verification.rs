//! Kani formal verification suite for the shared primitives (only built
//! under `cargo kani`).
//!
//! # Source
//!
//! `clamped_forward_euler`'s non-negativity is an algebraic fact about
//! IEEE `max`, not a literature claim; no reference key applies.

use crate::euler::clamped_forward_euler;

/// `clamped_forward_euler`, the saturation primitive
/// `(prev + dt * rate).max(0.0)`, is non-negative for a bounded finite
/// range of `prev` / `rate`, byte-for-byte the proof formerly housed in
/// the aps crate. The full `f64` domain property is an algebraic fact
/// about IEEE `max` (it returns the non-NaN operand when the other is
/// NaN and `+inf` stays `+inf`) rather than a solver result.
#[cfg(kani)]
#[kani::proof]
fn verify_physiological_non_negativity() {
    let prev: f64 = kani::any();
    let rate: f64 = kani::any();
    let dt: f64 = 1.0; // one minute, as in the specification

    // Realistic finite range (compartment masses are bounded; NaN and
    // infinities of a symbolic rate would trip Kani's float additions).
    kani::assume(prev >= -1_000.0 && prev <= 1_000.0);
    kani::assume(rate >= -1_000.0 && rate <= 1_000.0);

    let next = clamped_forward_euler(prev, rate, dt);

    kani::assert(
        next >= 0.0,
        "forward Euler saturation keeps compartments non-negative",
    );
}