//! Interacting multiple model (IMM) probability arithmetic, US8977504B2
//! eq 66-67 and 80. The filters, their mixing and the likelihood live in
//! [`crate::kalman`].

/// Number of models N. The patent gives none. src: own-imm
pub const IMM_MODE_COUNT: usize = 3;

/// Probability of staying in a model over one 15-min control period (about
/// 10 days per model; the likelihood still switches them). src: own-imm
pub const IMM_STAY: f64 = 0.999;

const IMM_SWITCH: f64 = (1.0 - IMM_STAY) / (IMM_MODE_COUNT - 1) as f64;

/// Markov transition matrix, `p[j][i]` the probability of switching from
/// model i to model j. The patent defines it without values.
pub const IMM_MARKOV_TRANSITION: [[f64; IMM_MODE_COUNT]; IMM_MODE_COUNT] = [
    [IMM_STAY, IMM_SWITCH, IMM_SWITCH],
    [IMM_SWITCH, IMM_STAY, IMM_SWITCH],
    [IMM_SWITCH, IMM_SWITCH, IMM_STAY],
];

/// Normalisation factors `c_j = sum_i p[j][i] mu[i]`, eq 67.
pub fn imm_prognostic_weights(mu: &[f64; IMM_MODE_COUNT]) -> [f64; IMM_MODE_COUNT] {
    std::array::from_fn(|j| {
        (0..IMM_MODE_COUNT)
            .map(|i| IMM_MARKOV_TRANSITION[j][i] * mu[i])
            .sum()
    })
}

/// Mixing probability `p[j][i] mu[i] / c_j`, eq 66. Zero when `c_j` is not
/// positive.
pub fn imm_mixing_probability(
    j: usize,
    i: usize,
    mu: &[f64; IMM_MODE_COUNT],
    c: &[f64; IMM_MODE_COUNT],
) -> f64 {
    if c[j] > 0.0 {
        IMM_MARKOV_TRANSITION[j][i] * mu[i] / c[j]
    } else {
        0.0
    }
}

/// Mode probability update `mu_j = c_j L_j / sum_m c_m L_m`, eq 80.
/// `None` when no mode explains the observation.
pub fn imm_mode_probability_update(
    likelihood: &[f64; IMM_MODE_COUNT],
    c: &[f64; IMM_MODE_COUNT],
) -> Option<[f64; IMM_MODE_COUNT]> {
    let total: f64 = c.iter().zip(likelihood).map(|(c, l)| c * l).sum();
    (total > 0.0).then(|| std::array::from_fn(|j| c[j] * likelihood[j] / total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-12, "{a} != {b}");
    }

    /// US8977504B2 eq 66, 67 and 80 on worked numbers.
    #[test]
    fn us504_imm_equations() {
        let p = IMM_MARKOV_TRANSITION;
        close(p[1][0], 0.0005);
        let mu = [0.5, 0.3, 0.2];
        let c = imm_prognostic_weights(&mu);
        close(c[0], p[0][0] * 0.5 + p[0][1] * 0.3 + p[0][2] * 0.2);
        close(c[1], p[1][0] * 0.5 + p[1][1] * 0.3 + p[1][2] * 0.2);
        close(imm_mixing_probability(1, 0, &mu, &c), p[1][0] * 0.5 / c[1]);
        let post = imm_mode_probability_update(&[1.0, 2.0, 4.0], &c).unwrap();
        let total = c[0] + 2.0 * c[1] + 4.0 * c[2];
        close(post[2], 4.0 * c[2] / total);
    }

    /// Zero weights don't divide by zero.
    #[test]
    fn zero_weights_are_left_alone() {
        let mu = [0.0; IMM_MODE_COUNT];
        let c = [0.0; IMM_MODE_COUNT];
        assert_eq!(imm_mixing_probability(0, 0, &mu, &c), 0.0);
        assert_eq!(imm_mode_probability_update(&[1.0; 3], &c), None);
    }
}
