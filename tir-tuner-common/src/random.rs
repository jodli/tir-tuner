//! Deterministic seeded RNG shared by the simulation crates.
//!
//! Simulation must reproduce a scenario end-to-end from its seed: the
//! same seed yields the same subject cohort, the same sensor noise
//! sequence and the same pump delivery error. No external chassis, no
//! system entropy.

/// SplitMix64-based deterministic generator.
pub struct SeededRng(u64);

impl SeededRng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Uniform in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        // SplitMix64; cast the top 53 bits to a mantissa.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Standard normal via Box-Muller.
    pub fn next_normal(&mut self) -> f64 {
        let u1 = (self.next_f64() + f64::MIN_POSITIVE).max(f64::MIN_POSITIVE);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }

    /// Normal with given mean and sdev.
    pub fn next_normal_mean(&mut self, mean: f64, sdev: f64) -> f64 {
        mean + sdev * self.next_normal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let mut a = SeededRng::new(7);
        let mut b = SeededRng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_f64(), b.next_f64());
            assert_eq!(a.next_normal(), b.next_normal());
        }
    }

    #[test]
    fn uniform_stays_in_unit_interval() {
        let mut rng = SeededRng::new(1);
        for _ in 0..10_000 {
            let v = rng.next_f64();
            assert!((0.0..1.0).contains(&v), "out of range: {v}");
        }
    }

    #[test]
    fn normal_close_to_standard_moments() {
        let mut rng = SeededRng::new(2);
        let n = 10_000;
        let mut sum = 0.0;
        let mut sq = 0.0;
        for _ in 0..n {
            let v = rng.next_normal();
            sum += v;
            sq += v * v;
        }
        let mean = sum / n as f64;
        let var = sq / n as f64 - mean * mean;
        assert!(mean.abs() < 0.05, "mean {}", mean);
        assert!((var - 1.0).abs() < 0.05, "variance {}", var);
    }

    #[test]
    fn seeded_runs_differ_between_seeds() {
        let mut a = SeededRng::new(1);
        let mut b = SeededRng::new(2);
        assert_ne!(a.next_f64(), b.next_f64());
    }
}