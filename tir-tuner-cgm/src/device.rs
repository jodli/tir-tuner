//! The sensor device: B08 eq 7 error on top of interstitial glucose.

use tir_tuner_common::random::SeededRng;
use tir_tuner_common::units::{MgDl, MmolL};

/// AR coefficient of the error driver per sample. src: b08-eq7
const AR_COEF: f64 = 0.7;
/// The sample interval eq 7 is fitted on (min). src: b08-interval
pub const SAMPLE_INTERVAL_MIN: f64 = 15.0;
/// Johnson SU λ, ξ, δ, γ (mg/dL scale). src: b08-johnson
const JOHNSON_SU: [f64; 4] = [15.96, -5.471, 1.6898, -0.5444];

/// How the sensor reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensorModel {
    /// B08 eq 7 error.
    Breton2008,
    /// No error, for tests that isolate the body or the controller.
    Exact,
}

/// One sensor, seeded so the error sequence is reproducible.
pub struct CgmSensor {
    model: SensorModel,
    /// The AR driver `e_n`; `None` before the first sample.
    driver: Option<f64>,
    rng: SeededRng,
}

impl CgmSensor {
    /// A fresh sensor; the first read starts eq 7 at `e_1 = v_1`.
    pub fn new(model: SensorModel, seed: u64) -> Self {
        Self {
            model,
            driver: None,
            rng: SeededRng::new(seed),
        }
    }

    /// One sample, `SAMPLE_INTERVAL_MIN` after the previous one.
    pub fn read(&mut self, interstitial: MmolL) -> MgDl {
        let error = match self.model {
            SensorModel::Exact => 0.0,
            SensorModel::Breton2008 => {
                let e = ar_step(self.driver, self.rng.next_normal());
                self.driver = Some(e);
                johnson(e)
            }
        };
        reading(interstitial.to_mg_dl().0 + error)
    }
}

/// B08 eq 7: `e_1 = v_1`, `e_n = 0.7 (e_{n-1} + v_n)`, `v_n ~ N(0, 1)`.
pub fn ar_step(previous: Option<f64>, v: f64) -> f64 {
    match previous {
        None => v,
        Some(e) => AR_COEF * (e + v),
    }
}

/// B08 eq 7: `ε_n = ξ + λ sinh((e_n - γ) / δ)`, the error in mg/dL.
fn johnson(e: f64) -> f64 {
    let [lambda, xi, delta, gamma] = JOHNSON_SU;
    xi + lambda * ((e - gamma) / delta).sinh()
}

/// Whole mg/dL, floored at 0. src: user-cgm-integers
pub fn reading(mg_dl: f64) -> MgDl {
    MgDl(mg_dl.round().max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(n: usize) -> Vec<f64> {
        let mut sensor = CgmSensor::new(SensorModel::Breton2008, 42);
        // At 100 mg/dL the floor never bites, so reading - 100 is the error
        // to the nearest mg/dL.
        let ig = MgDl(100.0).to_mmol_l();
        (0..n).map(|_| sensor.read(ig).0 - 100.0).collect()
    }

    #[test]
    fn b08_error_statistics() {
        let e = errors(100_000);
        let n = e.len() as f64;
        let m = e.iter().sum::<f64>() / n;
        let var: f64 = e.iter().map(|x| (x - m).powi(2)).sum();
        let sd = (var / n).sqrt();
        let lag = |k: usize| {
            e.windows(k + 1)
                .map(|w| (w[0] - m) * (w[k] - m))
                .sum::<f64>()
                / var
        };
        // B08 p6: mean 0.76, SD 11 mg/dL. Eq 7's driver has variance
        // 0.49/0.51, not 1, so the model gives about 0.7 and 11.7.
        assert!((m - 0.76).abs() < 0.3, "mean {m}");
        assert!((sd - 11.0).abs() < 1.0, "sd {sd}");
        // B08 p7 Fig 5B: PACF 0.7 at the 15-min lag, nothing after.
        assert!((lag(1) - 0.7).abs() < 0.03, "lag 1 {}", lag(1));
        assert!((lag(2) - 0.49).abs() < 0.03, "lag 2 {}", lag(2));
        // B08 eq 7: e_1 = v_1, then 0.7 (e + v). v is symmetric, so the
        // statistics above can't tell e + v from e - v.
        assert_eq!(ar_step(None, 0.3), 0.3);
        assert!((ar_step(Some(1.0), 0.5) - 1.05).abs() < 1e-12);
    }

    #[test]
    fn exact_sensor_rounds_and_floors() {
        let mut s = CgmSensor::new(SensorModel::Exact, 1);
        // 5 mmol/L is 90.08 mg/dL.
        assert_eq!(s.read(MmolL(5.0)), MgDl(90.0));
        assert_eq!(reading(90.5), MgDl(91.0));
        assert_eq!(reading(-3.0), MgDl(0.0));
    }
}
