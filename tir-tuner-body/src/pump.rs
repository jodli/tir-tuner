//! The pump hardware: delivery error and bolus speed.

use tir_tuner_common::random::SeededRng;

/// Delivery error CV, the same for infusion and bolus. src: w10-pump-error
pub const DELIVERY_ERROR_CV: f64 = 0.05;
/// YpsoPump bolus speed (U/min), 0.11 U/s. src: ypsopump-bolus-speed
const BOLUS_U_PER_MIN: f64 = 6.6;

/// Delivers infusions and boluses with W10's zero-mean uncorrelated error.
pub struct Pump {
    rng: SeededRng,
    error_cv: f64,
    pending_bolus_u: f64,
}

impl Pump {
    /// A pump with delivery error `error_cv`, seeded.
    pub fn new(seed: u64, error_cv: f64) -> Self {
        Self {
            rng: SeededRng::new(seed),
            error_cv,
            pending_bolus_u: 0.0,
        }
    }

    /// What is delivered for a programmed amount (a rate or a bolus).
    pub fn infusion(&mut self, amount: f64) -> f64 {
        (amount * self.rng.next_normal_mean(1.0, self.error_cv)).max(0.0)
    }

    /// Start a bolus of `units`; it runs at the YpsoPump's speed.
    pub fn bolus(&mut self, units: f64) {
        self.pending_bolus_u += self.infusion(units);
    }

    /// Bolus units delivered over the next `dt_min`.
    pub fn bolus_step(&mut self, dt_min: f64) -> f64 {
        let u = self.pending_bolus_u.min(BOLUS_U_PER_MIN * dt_min);
        self.pending_bolus_u -= u;
        u
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ypsopump_bolus_runs_at_0_11_u_per_s() {
        let mut p = Pump::new(1, 0.0);
        p.bolus(5.0);
        // 15 s steps: 1.65 U each, 5 U done in the fourth.
        let steps: Vec<f64> = (0..5).map(|_| p.bolus_step(0.25)).collect();
        for s in &steps[..3] {
            assert!((s - 0.11 * 15.0).abs() < 1e-12);
        }
        assert!((steps[3] - (5.0 - 3.0 * 1.65)).abs() < 1e-12);
        assert_eq!(steps[4], 0.0);
    }

    #[test]
    fn w10_error_is_zero_mean_with_5_percent_cv() {
        let mut p = Pump::new(7, DELIVERY_ERROR_CV);
        let n = 20_000;
        let d: Vec<f64> = (0..n).map(|_| p.infusion(1.0)).collect();
        let mean = d.iter().sum::<f64>() / n as f64;
        let sd = (d.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64).sqrt();
        assert!((mean - 1.0).abs() < 0.002, "mean {mean}");
        assert!((sd - 0.05).abs() < 0.002, "sd {sd}");
        p.bolus(4.0);
        let total: f64 = (0..10).map(|_| p.bolus_step(1.0)).sum();
        assert!(total != 4.0 && (total - 4.0).abs() < 1.0, "bolus {total}");
    }
}
