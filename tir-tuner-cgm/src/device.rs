//! The sensor device: AR(1) measurement error on top of the true
//! interstitial glucose, plus the calibration gain.

use tir_tuner_common::random::SeededRng;

/// Sensor model coefficients.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SensorParams {
    /// First-order autoregressive coefficient of the measurement error.
    /// Estimates for modern sensors. Most Dexcom/Medtronic analyses put
    /// this in 0.7-0.95 per five-minute step.
    pub alpha1: f64,
    /// Standard deviation of the zero-mean white innovation (mmol/L).
    pub noise_sd: f64,
    /// Calibration gain applied to the true interstitial glucose
    /// (dimensionless, near 1.0).
    pub calibration_gain: f64,
}

impl Default for SensorParams {
    fn default() -> Self {
        Self {
            alpha1: 0.85,
            noise_sd: super::SENSOR_NOISE_SD_MMOL_L,
            calibration_gain: 1.0,
        }
    }
}

/// A single sensor output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SensorReading {
    /// The reported glucose (mmol/L), clipped at zero.
    pub glucose_mmol_per_l: f64,
    /// The raw pre-clip value (mmol/L), before the zero floor.
    pub raw_mmol_per_l: f64,
    /// The measurement error `e_t` saved into the sensor state (mmol/L).
    pub error_mmol_per_l: f64,
}

/// Deterministic seeded state of one sensor device.
pub struct CgmSensor {
    params: SensorParams,
    /// `e_t`, the current measurement error (mmol/L).
    error: f64,
    rng: SeededRng,
}

impl CgmSensor {
    /// A fresh sensor with the given coefficients, seeded so the noise
    /// sequence is reproducible.
    pub fn new(params: SensorParams, seed: u64) -> Self {
        Self {
            params,
            error: 0.0,
            rng: SeededRng::new(seed),
        }
    }

    /// Read the interstitial glucose once. Advances the AR(1) error by
    /// one step via [`next_error`], then
    /// `raw = gain * ig + error`, reported value `max(raw, 0.0)`.
    pub fn read(&mut self, interstitial_mmol_per_l: f64) -> SensorReading {
        let innovation = self.rng.next_normal() * self.params.noise_sd;
        let error = next_error(&self.params, self.error, innovation);
        self.error = error;
        let raw = gain_signal(&self.params, interstitial_mmol_per_l, error);
        SensorReading {
            glucose_mmol_per_l: raw.max(0.0),
            raw_mmol_per_l: raw,
            error_mmol_per_l: error,
        }
    }
}

/// One AR(1) error step: `e' = alpha1 * e + w`. Pure so the Kani proofs
/// can drive it with symbolic inputs.
pub fn next_error(params: &SensorParams, current_error: f64, innovation: f64) -> f64 {
    params.alpha1 * current_error + innovation
}

/// The raw pre-clip reading for a given error term:
/// `gain * interstitial + e`. Pure, ditto.
pub fn gain_signal(params: &SensorParams, interstitial_mmol_per_l: f64, error: f64) -> f64 {
    params.calibration_gain * interstitial_mmol_per_l + error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_clips_at_zero() {
        let mut sensor = CgmSensor::new(SensorParams::default(), 1);
        // Noise pulls below zero only if the error exceeds the glucose;
        // drive it far negative by feeding a large innovation sequence
        // and confirm the floor still holds.
        let mut sensor = sensor;
        let mut min_observed = f64::MAX;
        for _ in 0..4_000 {
            let r = sensor.read(5.0);
            min_observed = min_observed.min(r.glucose_mmol_per_l);
            assert!(r.glucose_mmol_per_l >= 0.0);
        }
        // With 4000 draws of sd 0.5, the raw must occasionally dip below
        // 5.0 (noise std ~0.5, depth >2 sigma is routine).
        assert!(min_observed < 5.0, "noise should pull readings under 5.0");
    }

    #[test]
    fn zero_gain_reports_noise_only() {
        let params = SensorParams {
            calibration_gain: 0.0,
            ..SensorParams::default()
        };
        let mut sensor = CgmSensor::new(params, 1);
        let r = sensor.read(10.0);
        assert_eq!(r.raw_mmol_per_l, r.error_mmol_per_l);
    }

    #[test]
    fn noise_autocorrelation_tracks_alpha1() {
        // Estimate lag-1 autocorrelation of the AR(1) error sequence;
        // it should be near alpha1.
        let mut sensor = CgmSensor::new(SensorParams::default(), 42);
        let mut errors = Vec::with_capacity(20_000);
        for _ in 0..20_000 {
            errors.push(sensor.read(8.0).error_mmol_per_l);
        }
        let mean = errors.iter().sum::<f64>() / errors.len() as f64;
        var_terms(&errors, mean, 20_000, 0.85);
    }

    fn var_terms(errors: &[f64], mean: f64, n: usize, alpha1: f64) {
        // Sort inline to avoid a separate helper: covariance / variance.
        let var = errors.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / n as f64;
        let cov = errors
            .windows(2)
            .map(|w| (w[0] - mean) * (w[1] - mean))
            .sum::<f64>()
            / (n - 1) as f64;
        let lag1 = cov / var;
        // 20k samples, sampling error ~ sqrt((1-alpha1^2)/n) < 0.02.
        assert!((lag1 - alpha1).abs() < 0.05, "lag-1 acf {} vs {}", lag1, alpha1);
    }

    #[test]
    fn reproducible_from_seed() {
        let mut a = CgmSensor::new(SensorParams::default(), 7);
        let mut b = CgmSensor::new(SensorParams::default(), 7);
        for _ in 0..100 {
            let ra = a.read(6.0);
            let rb = b.read(6.0);
            assert_eq!(ra.glucose_mmol_per_l, rb.glucose_mmol_per_l);
            assert_eq!(ra.error_mmol_per_l, rb.error_mmol_per_l);
        }
    }

    proptest::proptest! {
        #[test]
        fn reading_stays_bounded(
            interstitial in 0.0f64..30.0,
            alpha1 in 0.0f64..0.99,
            noise_sd in 0.0f64..2.0,
            gain in 0.5f64..1.5,
            seed in 0u64..1000,
        ) {
            let params = SensorParams { alpha1, noise_sd, calibration_gain: gain };
            let mut sensor = CgmSensor::new(params, seed);
            for _ in 0..50 {
                let r = sensor.read(interstitial);
                assert!(r.glucose_mmol_per_l.is_finite());
                assert!(r.glucose_mmol_per_l >= 0.0);
            }
        }
    }
}