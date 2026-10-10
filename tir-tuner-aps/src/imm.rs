//! Interacting multiple model (IMM) probability arithmetic, US8977504B2
//! eq 65-82. Only the mode probabilities and mixtures: the per-mode Kalman
//! filters, their states and the likelihood (eq 79) aren't implemented, and
//! nothing in the simulation calls this yet.

/// Number of models N. The patent gives none. src: own-imm
pub const IMM_MODE_COUNT: usize = 3;

/// Normalize the mode probabilities in place. A mixture with non-positive
/// total weight stays as it is.
pub fn normalize_imm_probabilities(mu: &mut [f64; IMM_MODE_COUNT]) {
    let sum = imm_mixture_sum(mu);
    if sum > 0.0 {
        mu.iter_mut().for_each(|m| *m /= sum);
    }
}

/// Sum of the mode probabilities.
pub fn imm_mixture_sum(mu: &[f64; IMM_MODE_COUNT]) -> f64 {
    mu.iter().sum()
}

/// Markov transition matrix, `p[j][i]` the probability of switching from
/// model i to model j. The patent defines it without values; ours are
/// column-stochastic. src: own-imm
pub const IMM_MARKOV_TRANSITION: [[f64; IMM_MODE_COUNT]; IMM_MODE_COUNT] =
    [[0.95, 0.05, 0.05], [0.03, 0.90, 0.05], [0.02, 0.05, 0.90]];

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

/// Mixture mean `sum_j mu[j] x_j`, eq 81 for a scalar state.
pub fn imm_mixture_mean(values: &[f64; IMM_MODE_COUNT], mu: &[f64; IMM_MODE_COUNT]) -> f64 {
    mu.iter().zip(values).map(|(m, x)| m * x).sum()
}

/// Mixture variance `sum_j mu[j] (P_j + (x_j - x)^2)`, eq 82 for a scalar
/// state.
pub fn imm_mixture_variance(
    variances: &[f64; IMM_MODE_COUNT],
    means: &[f64; IMM_MODE_COUNT],
    mu: &[f64; IMM_MODE_COUNT],
    mixture_mean: f64,
) -> f64 {
    (0..IMM_MODE_COUNT)
        .map(|j| {
            let d = means[j] - mixture_mean;
            mu[j] * (variances[j] + d * d)
        })
        .sum()
}

/// Mode probability update `mu_j = c_j L_j / sum_m c_m L_m`, eq 80. All
/// zero when the normalizer is not positive.
pub fn imm_mode_probability_update(
    likelihood: &[f64; IMM_MODE_COUNT],
    c: &[f64; IMM_MODE_COUNT],
) -> [f64; IMM_MODE_COUNT] {
    let total: f64 = c.iter().zip(likelihood).map(|(c, l)| c * l).sum();
    if total > 0.0 {
        std::array::from_fn(|j| c[j] * likelihood[j] / total)
    } else {
        [0.0; IMM_MODE_COUNT]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-12, "{a} != {b}");
    }

    /// US8977504B2 eq 66, 67, 80-82 on worked numbers.
    #[test]
    fn us504_imm_equations() {
        let mu = [0.5, 0.3, 0.2];
        let c = imm_prognostic_weights(&mu);
        close(c[0], 0.95 * 0.5 + 0.05 * 0.3 + 0.05 * 0.2);
        close(c[1], 0.03 * 0.5 + 0.90 * 0.3 + 0.05 * 0.2);
        close(imm_mixing_probability(1, 0, &mu, &c), 0.03 * 0.5 / c[1]);
        let post = imm_mode_probability_update(&[1.0, 2.0, 4.0], &c);
        let total = c[0] + 2.0 * c[1] + 4.0 * c[2];
        close(post[2], 4.0 * c[2] / total);
        close(imm_mixture_mean(&[1.0, 2.0, 4.0], &mu), 0.5 + 0.6 + 0.8);
        let v = imm_mixture_variance(&[0.1, 0.2, 0.3], &[1.0, 2.0, 4.0], &mu, 1.9);
        close(
            v,
            0.5 * (0.1 + 0.81) + 0.3 * (0.2 + 0.01) + 0.2 * (0.3 + 4.41),
        );
    }

    #[test]
    fn normalize_scales_to_one() {
        let mut mu = [0.2, 0.2, 0.4];
        close(imm_mixture_sum(&mu), 0.8);
        normalize_imm_probabilities(&mut mu);
        close(mu[2], 0.5);
    }

    /// Zero weights stay zero instead of dividing by zero.
    #[test]
    fn zero_weights_are_left_alone() {
        let mut mu = [0.0; IMM_MODE_COUNT];
        normalize_imm_probabilities(&mut mu);
        assert_eq!(mu, [0.0; IMM_MODE_COUNT]);
        let c = [0.0; IMM_MODE_COUNT];
        assert_eq!(imm_mixing_probability(0, 0, &mu, &c), 0.0);
        assert_eq!(
            imm_mode_probability_update(&[1.0; 3], &c),
            [0.0; IMM_MODE_COUNT]
        );
    }
}
