//! Glucose estimation: the patent's interacting multiple model filter
//! (US8977504B2 eq 53-82) on its subset state `[q1, q2, q3, u_s]` (eq 46,
//! without the bioavailability F). The modes differ only in σ_w, the SD of
//! the change in unexplained glucose influx (eq 65): a quiet mode smooths
//! sensor noise, a loud one follows an unannounced meal.
//!
//! The patent takes the measurement noise as white (eq 62). The simulated
//! sensor's error is B08's AR(1), and a white-noise filter reads its runs
//! as glucose changes, so ours carries the error as a fifth state with
//! B08's coefficient and SD.
//!
//! The patent publishes the structure and no values: the σ_w per mode and
//! the transition matrix are ours.

use crate::hovorka::{HovorkaParams, HovorkaState};
use crate::imm::{
    IMM_MODE_COUNT, imm_mixing_probability, imm_mode_probability_update, imm_prognostic_weights,
};
use tir_tuner_common::units::MgDl;

/// SD of the CGM error (mg/dL). src: own-kf-sensor-sd
pub const SENSOR_ERROR_SD_MG_DL: f64 = 11.0;

/// Lag-1 correlation of the CGM error per control period. src: b08-eq7
pub const SENSOR_ERROR_AR: f64 = 0.7;

/// σ_w per mode (eq 65), per √min (mmol/kg/min). src: own-imm-process
pub const MODE_PROCESS_SD: [f64; IMM_MODE_COUNT] = [1e-5, 3e-4, 3e-3];

const Q1: usize = 0;
const Q2: usize = Q1 + 1;
const Q3: usize = Q2 + 1;
const US: usize = Q3 + 1;
/// The CGM error (mmol/L).
const ERR: usize = US + 1;
const DIM: usize = ERR + 1;

type Vector = [f64; DIM];
type Matrix = [[f64; DIM]; DIM];

/// The belief's subset state; the error isn't part of the belief.
fn sub(s: &HovorkaState) -> Vector {
    [s.q1, s.q2, s.q3, s.u_s, 0.0]
}

fn with_sub(s: HovorkaState, x: &Vector) -> HovorkaState {
    HovorkaState {
        q1: x[Q1].max(0.0),
        q2: x[Q2].max(0.0),
        q3: x[Q3].max(0.0),
        u_s: x[US],
        ..s
    }
}

/// One mode's estimate and covariance.
#[derive(Clone, Copy, Debug)]
struct ModeEstimate {
    x: Vector,
    cov: Matrix,
}

/// The belief plus one estimate per mode.
#[derive(Clone, Copy, Debug)]
pub struct Estimator {
    /// The combined belief the NMPC predicts from (eq 81).
    pub state: HovorkaState,
    modes: [ModeEstimate; IMM_MODE_COUNT],
    /// Mode probabilities μ (eq 80).
    mu: [f64; IMM_MODE_COUNT],
}

/// SD of the CGM error (mmol/L).
fn sensor_sd() -> f64 {
    MgDl(SENSOR_ERROR_SD_MG_DL).to_mmol_l().0
}

impl Estimator {
    /// Start at `state` with glucose known to the sensor SD and `u_s` to
    /// be zero: the patent starts from the first measurement (eq 63-64).
    /// Every mode is equally likely.
    pub fn new(params: &HovorkaParams, state: HovorkaState) -> Self {
        let sd = sensor_sd();
        // One glucose error moves all three masses, at eq 63's ratio.
        let mut shifted = state;
        shifted.shift_glucose(params, sd);
        let (x, x_sd) = (sub(&state), sub(&shifted));
        let dir: Vector = std::array::from_fn(|i| x_sd[i] - x[i]);
        let mut cov = outer(&dir, &dir);
        cov[ERR][ERR] = sd * sd;
        Self {
            state,
            modes: [ModeEstimate { x, cov }; IMM_MODE_COUNT],
            mu: [1.0 / IMM_MODE_COUNT as f64; IMM_MODE_COUNT],
        }
    }

    /// Mode probabilities, quiet mode first.
    pub fn mode_probabilities(&self) -> [f64; IMM_MODE_COUNT] {
        self.mu
    }

    /// Interact (eq 66-69), then predict every mode over one control
    /// period of `dt_min` (eq 53-59, 70-71). `propagate` steps the belief
    /// with the known inputs and holds `u_s`, as the period ends in a
    /// measurement (eq 58). It is affine in the subset states, so unit
    /// steps give its exact Jacobian `F` (eq 55), shared by every mode.
    pub fn predict(&mut self, dt_min: f64, propagate: impl Fn(HovorkaState) -> HovorkaState) {
        let c = imm_prognostic_weights(&self.mu);
        let mixed: [ModeEstimate; IMM_MODE_COUNT] = std::array::from_fn(|j| {
            let w: [f64; IMM_MODE_COUNT] =
                std::array::from_fn(|i| imm_mixing_probability(j, i, &self.mu, &c));
            mixture(&self.modes, &w)
        });

        let x0 = sub(&self.state);
        let next = propagate(self.state);
        let fx = sub(&next);
        let mut f = [[0.0; DIM]; DIM];
        for j in Q1..ERR {
            let mut x = x0;
            x[j] += 1.0;
            let fj = sub(&propagate(with_sub(self.state, &x)));
            for (row, (a, b)) in f.iter_mut().zip(fj.iter().zip(&fx)) {
                row[j] = a - b;
            }
        }
        f[ERR][ERR] = SENSOR_ERROR_AR;
        // Eq 56 and 59: the u_s shock enters where a unit u_s does.
        let g: Vector = std::array::from_fn(|i| f[i][US]);
        let sd = sensor_sd();
        let err_q = sd * sd * (1.0 - SENSOR_ERROR_AR * SENSOR_ERROR_AR);

        for ((m, mix), sd) in self.modes.iter_mut().zip(&mixed).zip(MODE_PROCESS_SD) {
            let dx: Vector = std::array::from_fn(|i| mix.x[i] - x0[i]);
            m.x = std::array::from_fn(|i| fx[i] + dot(&f[i], &dx));
            let fp = mul(&f, &mix.cov);
            let q = sd * sd * dt_min;
            m.cov = std::array::from_fn(|i| {
                std::array::from_fn(|j| dot(&fp[i], &f[j]) + q * g[i] * g[j])
            });
            m.cov[ERR][ERR] += err_q;
        }
        // The combined subset waits for the update; the rest moves on.
        self.state = next;
    }

    /// Update every mode with a CGM reading in mmol/L (eq 60, 72-77): the
    /// reading is interstitial glucose `q3 / V_G` (eq 61) plus the error
    /// state. Weigh the modes by their likelihood (eq 79-80) and combine
    /// them (eq 81).
    pub fn update(&mut self, params: &HovorkaParams, reading: f64) {
        let mut h = [0.0; DIM];
        h[Q3] = 1.0 / params.v_g;
        h[ERR] = 1.0;
        let mut likelihood = [0.0; IMM_MODE_COUNT];
        for (m, l) in self.modes.iter_mut().zip(&mut likelihood) {
            let y = reading - dot(&h, &m.x);
            let ph: Vector = std::array::from_fn(|i| dot(&m.cov[i], &h));
            let s = dot(&h, &ph);
            let k = ph.map(|v| v / s);
            m.x = std::array::from_fn(|i| m.x[i] + k[i] * y);
            m.cov = std::array::from_fn(|i| std::array::from_fn(|j| m.cov[i][j] - k[i] * ph[j]));
            *l = (-y * y / (s + s)).exp() / (std::f64::consts::TAU * s).sqrt();
        }
        // The c of this cycle's interaction: mu hasn't changed since.
        let c = imm_prognostic_weights(&self.mu);
        // A reading no mode explains leaves the probabilities as they were.
        if let Some(mu) = imm_mode_probability_update(&likelihood, &c) {
            self.mu = mu;
        }
        let combined = mixture(&self.modes, &self.mu);
        self.state = with_sub(self.state, &combined.x);
    }
}

/// Weighted mean and covariance of the mode estimates, eq 68-69 and 81-82.
fn mixture(modes: &[ModeEstimate; IMM_MODE_COUNT], w: &[f64; IMM_MODE_COUNT]) -> ModeEstimate {
    let x: Vector = std::array::from_fn(|i| modes.iter().zip(w).map(|(m, w)| w * m.x[i]).sum());
    let cov = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            modes
                .iter()
                .zip(w)
                .map(|(m, w)| w * (m.cov[i][j] + (m.x[i] - x[i]) * (m.x[j] - x[j])))
                .sum()
        })
    });
    ModeEstimate { x, cov }
}

fn dot(a: &Vector, b: &Vector) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn outer(a: &Vector, b: &Vector) -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| a[i] * b[j]))
}

fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..DIM).map(|k| a[i][k] * b[k][j]).sum()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> HovorkaParams {
        HovorkaParams::from_weight_and_tdd(70.0, 50.0).resting_at(6.0)
    }

    /// Eq 72-76 with the reading as glucose plus sensor error: both start
    /// equally uncertain, so each takes half of a surprise.
    #[test]
    fn us504_update_splits_the_innovation_by_variance() {
        let p = params();
        let mut e = Estimator::new(&p, HovorkaState::basal(&p, 6.0));
        e.update(&p, 7.0);
        assert!((e.state.interstitial_glucose(&p) - 6.5).abs() < 1e-9);
        // The plasma-interstitial gradient stays (eq 63's ratio).
        assert!((e.state.plasma_glucose(&p) - 6.5).abs() < 1e-9);
        assert!((e.state.q2 - 6.5 * p.v_g * p.k21 / p.k12).abs() < 1e-9);
        let err = mixture(&e.modes, &e.mu).x[ERR];
        assert!((err - 0.5).abs() < 1e-9);
    }

    /// Eq 77: the update halves the glucose variance, which starts at the
    /// sensor's 11 mg/dL.
    #[test]
    fn us504_update_halves_the_variance() {
        let p = params();
        let mut e = Estimator::new(&p, HovorkaState::basal(&p, 6.0));
        e.update(&p, 6.0);
        let var = mixture(&e.modes, &e.mu).cov[Q3][Q3] / (p.v_g * p.v_g);
        let sd = 11.0 / 18.0156;
        assert!((var - sd * sd / 2.0).abs() < 1e-12, "{var}");
    }

    /// Eq 71 with F = I: the glucose variance carries over, the B08 error
    /// stays at its stationary variance, and only u_s takes process noise.
    #[test]
    fn us504_predict_propagates_the_covariance() {
        let p = params();
        let mut e = Estimator::new(&p, HovorkaState::basal(&p, 6.0));
        let before = e.modes[0].cov;
        e.predict(15.0, |b| b);
        let m = e.modes[0];
        assert!((m.cov[Q3][Q3] - before[Q3][Q3]).abs() < 1e-15);
        assert!((m.cov[ERR][ERR] - before[ERR][ERR]).abs() < 1e-15);
        let sd = MODE_PROCESS_SD[0];
        assert!((m.cov[US][US] - sd * sd * 15.0).abs() < 1e-20);
    }

    /// Eq 69 and 82: the mixture adds the spread of the means.
    #[test]
    fn us504_mixture_covariance() {
        let mut a = ModeEstimate {
            x: [0.0; DIM],
            cov: [[0.0; DIM]; DIM],
        };
        a.cov[Q1][Q1] = 1.0;
        let mut b = a;
        b.x[Q1] = 2.0;
        b.x[Q2] = 4.0;
        b.cov[Q1][Q1] = 3.0;
        let m = mixture(&[a, b, b], &[0.5, 0.5, 0.0]);
        assert!((m.x[Q1] - 1.0).abs() < 1e-12);
        // 0.5 (1 + 1) + 0.5 (3 + 1)
        assert!((m.cov[Q1][Q1] - 3.0).abs() < 1e-12);
        // 0.5 (-1)(-2) + 0.5 (1)(2)
        assert!((m.cov[Q1][Q2] - 2.0).abs() < 1e-12);
    }
}
