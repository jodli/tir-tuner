//! Interacting Multiple Model (IMM) mode-probability bookkeeping.
//!
//! `N` parallel filters (modes) each carry a mix probability `mu`.  The
//! suite verifies that normalization keeps the mixture a well-formed
//! probability distribution: each mode probability remains in `[0, 1]`
//! and the mixture sums to `1.0` within floating-point tolerance.

/// Number of parallel filter modes.
pub const IMM_MODE_COUNT: usize = 3;

/// Accepted deviation of the normalized mode-probability sum from 1.0.
pub const IMM_PROBABILITY_SUM_TOLERANCE: f64 = 1e-6;

/// Normalize the mode probabilities in place so they form a valid
/// probability distribution.
///
/// When the input mixture has non-positive total weight the mixture is
/// left untouched. For a positive-weight, non-negative input every entry
/// lands in `[0, 1]` and the mixture sums to one up to floating-point
/// rounding, which the Kani suite verifies.
pub fn normalize_imm_probabilities(mu: &mut [f64; IMM_MODE_COUNT]) {
    let sum = mu[0] + mu[1] + mu[2];
    if sum > 0.0 {
        let reciprocal = 1.0 / sum;
        mu[0] *= reciprocal;
        mu[1] *= reciprocal;
        mu[2] *= reciprocal;
    }
}

/// Mixture sum of the mode probabilities after normalization.
pub fn imm_mixture_sum(mu: &[f64; IMM_MODE_COUNT]) -> f64 {
    mu[0] + mu[1] + mu[2]
}

/// Markov transition matrix `p[j][i]` of the IMM model, the probability
/// of being in mode `j` at the current step given mode `i` at the
/// previous step. Column-stochastic: every column sums to one.
///
/// The matrix **structure** (one diagonal-dominant column-stochastic
/// matrix, mixing equations per section 4.1.1) follows patent
/// CA2702345C [102-108]; the concrete entries are illustrative tuning
/// values, not published algorithm internals, and may be treated as
/// tunable for the in-silico work. The verification properties only rely
/// on column stochasticity and non-negativity, not on the specific
/// entries.
pub const IMM_MARKOV_TRANSITION: [[f64; IMM_MODE_COUNT]; IMM_MODE_COUNT] =
    [[0.95, 0.05, 0.05], [0.03, 0.90, 0.05], [0.02, 0.05, 0.90]];

/// Prognostic weights `c_j = sum_i p[j][i] * mu[i]` (section 4.1.1).
///
/// With a column-stochastic transition matrix and a non-negative
/// normalized `mu` the weights are between 0 and 1 and sum to one over
/// `j`; the Kani suite proves normalization of the mixing probabilities
/// derived from them.
pub fn imm_prognostic_weights(mu: &[f64; IMM_MODE_COUNT]) -> [f64; IMM_MODE_COUNT] {
    let mut c = [0.0; IMM_MODE_COUNT];
    for j in 0..IMM_MODE_COUNT {
        for i in 0..IMM_MODE_COUNT {
            c[j] += IMM_MARKOV_TRANSITION[j][i] * mu[i];
        }
    }
    c
}

/// Mixing probability `mu_{i|j} = p[j][i] * mu[i] / c_j` (section 4.1.1).
///
/// Only valid when `c[j] > 0.0`; otherwise any mixing is undefined and
/// zero is returned. For a fixed `j` the mixing probabilities sum to one
/// over `i`, which the Kani suite proves.
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

/// Mixture-weighted mean of the per-mode values (section 4.1.5).
///
/// For non-negative weights summing to (about) one this stays inside the
/// convex hull of the per-mode values, which the Kani suite proves.
pub fn imm_mixture_mean(values: &[f64; IMM_MODE_COUNT], mu: &[f64; IMM_MODE_COUNT]) -> f64 {
    mu[0] * values[0] + mu[1] * values[1] + mu[2] * values[2]
}

/// Mixture-weighted covariance for scalar per-mode state estimates
/// (section 4.1.5): `sum_j mu[j] * (P_j + (x_j - x)(x_j - x))`. Every
/// term is a variance, so the mixture is non-negative.
pub fn imm_mixture_variance(
    variances: &[f64; IMM_MODE_COUNT],
    means: &[f64; IMM_MODE_COUNT],
    mu: &[f64; IMM_MODE_COUNT],
    mixture_mean: f64,
) -> f64 {
    mu[0] * (variances[0] + (means[0] - mixture_mean).powi(2))
        + mu[1] * (variances[1] + (means[1] - mixture_mean).powi(2))
        + mu[2] * (variances[2] + (means[2] - mixture_mean).powi(2))
}

/// Bayesian mode-probability update (section 4.1.4):
/// `mu_j = c_j * Lambda_j / sum_m c_m * Lambda_m`.
///
/// For non-negative likelihood values with a positive normalizer the
/// result is a valid distribution, which the Kani suite proves. When the
/// normalizer is non-positive the posterior is left unchanged (all
/// zero), mirroring the guard in [`normalize_imm_probabilities`].
pub fn imm_mode_probability_update(
    likelihood: &[f64; IMM_MODE_COUNT],
    c: &[f64; IMM_MODE_COUNT],
) -> [f64; IMM_MODE_COUNT] {
    let mut post = [0.0; IMM_MODE_COUNT];
    let total = c[0] * likelihood[0] + c[1] * likelihood[1] + c[2] * likelihood[2];
    if total > 0.0 {
        for j in 0..IMM_MODE_COUNT {
            post[j] = c[j] * likelihood[j] / total;
        }
    }
    post
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Concrete sweep over the exact-arithmetic lattice: normalization
    /// keeps the mixture sum within `IMM_PROBABILITY_SUM_TOLERANCE` of
    /// 1.0 for every sampled point (stride 8 over `n in 0..=1024`, plus
    /// all edge weights). This complements the symbolic Kani proof, which
    /// discharges the same identity over the lattice for arbitrary
    /// symbolic mantissas.
    #[test]
    fn normalization_sums_to_one_on_lattice() {
        let lattice = 1024u32;
        let mut max_deviation: f64 = 0.0;

        let mut probe = |a: u32, b: u32, c: u32| {
            let mut mu: [f64; 3] = [
                a as f64 / lattice as f64,
                b as f64 / lattice as f64,
                c as f64 / lattice as f64,
            ];
            let sum = mu[0] + mu[1] + mu[2];
            if sum <= 0.001 {
                return;
            }
            normalize_imm_probabilities(&mut mu);
            let deviation = (imm_mixture_sum(&mu) - 1.0).abs();
            max_deviation = max_deviation.max(deviation);

            for normalized in mu {
                assert!((0.0..=1.0).contains(&normalized), "entry is a probability");
            }
        };

        let mut n0 = 0u32;
        while n0 <= lattice {
            let mut n1 = 0u32;
            while n1 <= lattice {
                probe(n0, n1, 0);
                probe(n0, n1, lattice);
                let mut n2 = 8u32;
                while n2 <= lattice {
                    probe(n0, n1, n2);
                    n2 += 8;
                }
                n1 += 8;
            }
            n0 += 8;
        }

        assert!(
            max_deviation < IMM_PROBABILITY_SUM_TOLERANCE,
            "max normalized-sum deviation {max_deviation} exceeds tolerance"
        );
    }

    /// Concrete sweep complementing the symbolic IMM-cycle Kani proofs:
    /// for every lattice point with positive prognostic weights the
    /// mixing probabilities over `i` sum to one per mode and the
    /// Bayesian update re-normalizes the mode probabilities.
    #[test]
    fn cycle_normalizes_on_lattice() {
        let lattice = 16u32;
        let likelihood = [1.0, 2.0, 3.0];
        let mut max_mixing_deviation: f64 = 0.0;
        let mut max_bayes_deviation: f64 = 0.0;

        let mut n0 = 0u32;
        while n0 <= lattice {
            let mut n1 = 0u32;
            while n1 <= lattice {
                let mut n2 = 0u32;
                while n2 <= lattice {
                    let mu = [
                        n0 as f64 / lattice as f64,
                        n1 as f64 / lattice as f64,
                        n2 as f64 / lattice as f64,
                    ];
                    let c = imm_prognostic_weights(&mu);
                    if c.iter().any(|w| *w <= 0.001) {
                        n2 += 1;
                        continue;
                    }
                    for j in 0..IMM_MODE_COUNT {
                        let mut mixing_sum = 0.0;
                        for i in 0..IMM_MODE_COUNT {
                            mixing_sum += imm_mixing_probability(j, i, &mu, &c);
                        }
                        max_mixing_deviation = max_mixing_deviation.max((mixing_sum - 1.0).abs());
                    }
                    let post = imm_mode_probability_update(&likelihood, &c);
                    max_bayes_deviation =
                        max_bayes_deviation.max((imm_mixture_sum(&post) - 1.0).abs());
                    n2 += 1;
                }
                n1 += 1;
            }
            n0 += 1;
        }

        assert!(
            max_mixing_deviation < IMM_PROBABILITY_SUM_TOLERANCE,
            "max mixing-sum deviation {max_mixing_deviation} exceeds tolerance"
        );
        assert!(
            max_bayes_deviation < IMM_PROBABILITY_SUM_TOLERANCE,
            "max bayes-sum deviation {max_bayes_deviation} exceeds tolerance"
        );
    }

    /// Combination step (section 4.1.5) on the exact-arithmetic lattice
    /// with weights summing to exactly one (`n0 + n1 + n2 == LAT`): the
    /// mixture-weighted mean stays inside the convex hull of the per-mode
    /// values and the mixture variance is never negative.
    #[test]
    fn mixture_convex_hull_and_variance_on_lattice() {
        let lattice = 16u32;
        let x_vals = [0.0, 5.0, 10.0, 30.0];
        let p_vals = [0.0, 0.1, 1.0, 10.0];
        let mut n0 = 0u32;
        while n0 <= lattice {
            let mut n1 = 0u32;
            while n1 <= lattice {
                let n2 = lattice as i64 - n0 as i64 - n1 as i64;
                if n2 < 0 || n2 > lattice as i64 {
                    n1 += 1;
                    continue;
                }
                let n2 = n2 as u32;
                let mu = [
                    n0 as f64 / lattice as f64,
                    n1 as f64 / lattice as f64,
                    n2 as f64 / lattice as f64,
                ];
                let sum = imm_mixture_sum(&mu);
                assert!((sum - 1.0).abs() < 1e-12, "weights sum to one");
                for &x0 in &x_vals {
                    for &x1 in &x_vals {
                        for &x2 in &x_vals {
                            let mean = imm_mixture_mean(&[x0, x1, x2], &mu);
                            let lo = x0.min(x1).min(x2);
                            let hi = x0.max(x1).max(x2);
                            assert!(
                                mean >= lo - 1e-12 && mean <= hi + 1e-12,
                                "mixture mean leaves the convex hull: {mean}"
                            );
                            for &p0 in &p_vals {
                                let variance =
                                    imm_mixture_variance(&[p0, 0.0, 0.0], &[x0, x1, x2], &mu, mean);
                                assert!(
                                    variance >= -1e-12,
                                    "mixture variance went negative: {variance}"
                                );
                            }
                        }
                    }
                }
                n1 += 1;
            }
            n0 += 1;
        }
    }

    /// Property suite complementing the symbolic normalization sign proof
    /// and the exhaustive lattice sweeps above: over arbitrary `f64`
    /// mixtures the normalized entries stay in `[0, 1]`, the mixture
    /// sums to one, the mixing probabilities form a distribution per
    /// mode, and the Bayesian update returns a distribution.
    #[cfg(test)]
    mod prop_tests {
        use super::*;
        use crate::test_support::config;
        use proptest::prelude::*;

        /// Probability entries may exceed `1.0` by a rounding ulp; the
        /// tolerance is far below `IMM_PROBABILITY_SUM_TOLERANCE`.
        fn in_unit_interval(value: f64) -> bool {
            (-1e-12..=1.0 + 1e-12).contains(&value)
        }

        proptest! {
            #![proptest_config(config())]

            #[test]
            fn normalize_bounds_and_sums_to_one(
                m0 in 0.0..1.0f64,
                m1 in 0.0..1.0f64,
                m2 in 0.0..1.0f64,
            ) {
                prop_assume!(m0 + m1 + m2 > 0.001);
                let mut mu = [m0, m1, m2];
                normalize_imm_probabilities(&mut mu);
                for value in mu {
                    prop_assert!(in_unit_interval(value), "entry is a probability: {value}");
                }
                let sum = imm_mixture_sum(&mu);
                prop_assert!(
                    (sum - 1.0).abs() < IMM_PROBABILITY_SUM_TOLERANCE,
                    "normalized sum is {sum}"
                );
            }

            #[test]
            fn mixing_probabilities_are_distribution(
                m0 in 0.0..1.0f64,
                m1 in 0.0..1.0f64,
                m2 in 0.0..1.0f64,
            ) {
                let mu = [m0, m1, m2];
                let c = imm_prognostic_weights(&mu);
                prop_assume!(c.iter().all(|w| *w > 0.001));
                for j in 0..IMM_MODE_COUNT {
                    let mut sum = 0.0;
                    for i in 0..IMM_MODE_COUNT {
                        let p = imm_mixing_probability(j, i, &mu, &c);
                        prop_assert!(in_unit_interval(p), "mixing probability out of [0, 1]: {p}");
                        sum += p;
                    }
                    prop_assert!(
                        (sum - 1.0).abs() < IMM_PROBABILITY_SUM_TOLERANCE,
                        "mixing sum for mode {j} is {sum}"
                    );
                }
            }

            #[test]
            fn mode_update_is_distribution(
                m0 in 0.0..1.0f64,
                m1 in 0.0..1.0f64,
                m2 in 0.0..1.0f64,
                l0 in 0.0..10.0f64,
                l1 in 0.0..10.0f64,
                l2 in 0.0..10.0f64,
            ) {
                let mu = [m0, m1, m2];
                let c = imm_prognostic_weights(&mu);
                prop_assume!(c.iter().all(|w| *w > 0.001));
                let likelihood = [l0, l1, l2];
                prop_assume!(likelihood.iter().any(|l| *l > 0.0));
                let post = imm_mode_probability_update(&likelihood, &c);
                for value in post {
                    prop_assert!(in_unit_interval(value), "posterior out of [0, 1]: {value}");
                }
                let sum = imm_mixture_sum(&post);
                prop_assert!(
                    (sum - 1.0).abs() < IMM_PROBABILITY_SUM_TOLERANCE,
                    "posterior sum is {sum}"
                );
            }
        }
    }
}
