//! Numerical integration primitives.

/// Forward Euler update saturated at zero: `(prev + dt * rate).max(0.0)`.
///
/// This is the enforcement primitive behind the physiological
/// non-negativity invariants of the glucoregulatory models. For every
/// `f64` input the result is non-negative: IEEE `max` returns the non-NaN
/// operand when the other is NaN (clamping NaN rates to zero) and a
/// `+inf` rate is returned unchanged, still `>= 0.0`. Kani discharges
/// the negation only over a bounded finite range of `prev` / `rate`;
/// the full-domain statement is an algebraic property of IEEE `max`,
/// not a solver result.
pub fn clamped_forward_euler(prev: f64, rate: f64, dt: f64) -> f64 {
    (prev + dt * rate).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturates_at_zero() {
        assert_eq!(clamped_forward_euler(5.0, -10.0, 1.0), 0.0);
        assert_eq!(clamped_forward_euler(0.0, -1.0, 0.25), 0.0);
    }

    #[test]
    fn updates_incrementally() {
        assert_eq!(clamped_forward_euler(5.0, 2.0, 0.25), 5.5);
    }

    #[test]
    fn clamps_nan_rate_to_zero() {
        assert_eq!(clamped_forward_euler(0.0, f64::NAN, 1.0), 0.0);
    }
}