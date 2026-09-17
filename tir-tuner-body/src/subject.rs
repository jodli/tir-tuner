//! Virtual subject parameterization (Wilinska et al. 2010, Tables 1-2).
//!
//! A [`VirtualSubject`] bundles the fixed published parameter values with
//! a deterministic sample of the population distributions, so every
//! subject is reproducible from its seed. The country of the distributions
//! comes from the paper; the sampler here is a straightforward seeded
//! normal/uniform draw over the published moments (see `cohort`).
//!
//! # Model divergences from the published text
//!
//! * EGP: Wilinska prints `EGP = EGP0[1+x3]`, which would raise liver
//!   glucose output as insulin action grows. Insulin suppresses EGP, so
//!   the sign is treated as a typo. This crate uses the exponential
//!   suppression `egp0 * 2^((i_basal - x3)/0.5)`, capped at 3x basal EGP,
//!   the same philosophy as the aps crate and the physiologically
//!   intended direction.
//! * Renal excretion is applied to the accessible glucose `G` through
//!   the `R_cl (G - R_thr) VG` form, not the piecewise-linear Hovorka
//!   original.
//! * Gut absorption carries the meal bioavailability on the input side
//!   and clamps the appearance rate at `UG_ceil` (per-AUC discretized
//!   with a `bio` draw as published).
//!
//! Compartment conventions (matching the Cambridge simulator):
//! subcutaneous insulin masses `s1`/`s2` in mU, plasma insulin `i` and
//! actions `x1`/`x2`/`x3` in mU/L, glucose masses `q1`/`q2` in mmol/kg,
//! gut stores `g1`/`g2` in mmol, interstitial glucose `c` in mmol/L.
//! Insulin concentrations are read in U/L; the ingestion input is grams
//! of carbohydrate per minute.

use tir_tuner_common::random::SeededRng;
use tir_tuner_common::units::MU_PER_UNIT;

/// Parameters of a virtual subject.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VirtualSubject {
    /// Body weight (kg). Population 74.9 +/- 14.4.
    pub weight_kg: f64,
    /// Basal insulin requirement (U/h). Population daily dose 0.35 +/-
    /// 0.14 U/kg/d, scaled to the day.
    pub bir_u_per_h: f64,
    /// Carbohydrate-to-insulin ratio (U per 10 g). Population 1.7 +/- 1.0.
    pub icr_u_per_10g_cho: f64,

    /// Glucose distribution volume (L/kg). Sampled log-normal around 0.15.
    pub vg_l_per_kg: f64,
    /// Insulin distribution volume (L/kg). Normal 0.12 +/- 0.012.
    pub vi_l_per_kg: f64,
    /// Insulin clearance rate (1/min). Normal 0.14 +/- 0.035.
    pub ke_per_min: f64,
    /// Subcutaneous insulin absorption rate (1/min). Normal 0.018 +/-
    /// 0.0045.
    pub ka_per_min: f64,
    /// Rate constant for gut-to-plasma inter-compartment transfer (1/min).
    /// Fixed 0.060.
    pub k12_per_min: f64,

    /// Insulin action gain for glucose transport (units of x1 per mU/L).
    /// Fixed 18.41e-4.
    pub sit_per_mu_l: f64,
    /// Insulin action gain for glucose disposal (units of x2 per mU/L).
    /// Fixed 5.05e-4.
    pub sid_per_mu_l: f64,
    /// Insulin action gain for EGP suppression (units of x3 per mU/L).
    /// Fixed 0.019.
    pub sie_per_mu_l: f64,
    /// Rate constant for x1 approach to equilibrium (1/min). Fixed 0.0034.
    pub kb1_per_min: f64,
    /// Rate constant for x2 approach to equilibrium (1/min). Fixed 0.056.
    pub kb2_per_min: f64,
    /// Rate constant for x3 approach to equilibrium (1/min). Fixed 0.024.
    pub kb3_per_min: f64,

    /// Non-insulin-dependent glucose uptake at basal glucose (mmol/kg/min).
    /// Fixed 11.1 / 1000.
    pub f01_mmol_per_kg_min: f64,
    /// Basal endogenous glucose production (mmol/kg/min). Fixed 16.9 / 1000.
    pub egp0_mmol_per_kg_min: f64,
    /// Renal glucose excretion threshold (mmol/L). Normal 9 +/- 1.5.
    pub r_thr_mmol_per_l: f64,
    /// Renal glucose excretion rate (1/min). Normal 0.01 +/- 0.025,
    /// clamped to non-negative.
    pub r_cl_per_min: f64,

    /// Meal bioavailability fraction, 0.70-1.20 (100% + 20%). Intra-subject
    /// variation is applied as an extra +-20% draw in the engine.
    pub bio_fraction: f64,
    /// Peak gut absorption time (min). Log-normal median exp(3.689) ~ 40.
    pub t_max_g_min: f64,
    /// Upper cap on the gut glucose appearance rate (mmol/kg/min),
    /// uniform 0.02-0.035.
    pub ug_ceil_mmol_per_kg_min: f64,
    /// Interstitial glucose equilibration rate (1/min). Log-normal median
    /// exp(-2.372) ~ 0.093.
    pub ka_int_per_min: f64,
}

impl VirtualSubject {
    /// The population-mean subject: every parameter at its published
    /// typical value, no sampling. Used as the deterministic default.
    pub fn population_mean() -> Self {
        Self {
            weight_kg: 74.9,
            bir_u_per_h: 0.35 * 74.9 / 24.0, // 1.09 U/h
            icr_u_per_10g_cho: 1.7,
            vg_l_per_kg: 0.15,
            vi_l_per_kg: 0.12,
            ke_per_min: 0.14,
            ka_per_min: 0.018,
            k12_per_min: 0.060,
            sit_per_mu_l: 18.41e-4,
            sid_per_mu_l: 5.05e-4,
            sie_per_mu_l: 0.019,
            kb1_per_min: 0.0034,
            kb2_per_min: 0.056,
            kb3_per_min: 0.024,
            f01_mmol_per_kg_min: 11.1e-3,
            egp0_mmol_per_kg_min: 16.9e-3,
            r_thr_mmol_per_l: 9.0,
            r_cl_per_min: 0.01,
            bio_fraction: 1.0,
            t_max_g_min: 40.0,
            ug_ceil_mmol_per_kg_min: 0.028,
            ka_int_per_min: 0.093,
        }
    }

    /// Basal steady-state plasma insulin concentration (mU/L) for the
    /// subject's basal requirement.
    ///
    /// From the insulin ODEs at steady state: `s1 = s2 = u/ka`,
    /// `i = ka*s2/(vi*w*ke) = u/(vi*w*ke)`, with `u` the basal mass
    /// rate (mU/min).
    pub fn basal_insulin_concentration(&self) -> f64 {
        let u_per_min = self.bir_u_per_h / 60.0 * MU_PER_UNIT;
        u_per_min / (self.vi_l_per_kg * self.weight_kg * self.ke_per_min)
    }

    /// Basal steady-state value of insulin action `x3` (mU/L), used as
    /// the EGP anchor.
    pub fn basale_x3(&self) -> f64 {
        self.sie_per_mu_l * self.basal_insulin_concentration()
    }
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

fn log_normal(rng: &mut SeededRng, mean_ln: f64, sdev_ln: f64) -> f64 {
    (mean_ln + sdev_ln * rng.next_normal()).exp()
}

/// A deterministic cohort of `n` subjects drawn from the published
/// population distributions, seeded so the same `seed` always yields the
/// same subjects. Bounded parameters are clamped to physiological range.
pub fn cohort(n: usize, seed: u64) -> Vec<VirtualSubject> {
    let premium = VirtualSubject::population_mean();
    let mut rng = SeededRng::new(seed);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let weight = clamp(rng.next_normal_mean(74.9, 14.4), 30.0, 150.0);
        // Daily dose 0.35 +/- 0.14 U/kg/d, clamped positive.
        let daily_per_kg = (rng.next_normal_mean(0.35, 0.14)).max(0.05);
        out.push(VirtualSubject {
            weight_kg: weight,
            bir_u_per_h: daily_per_kg * weight / 24.0,
            icr_u_per_10g_cho: clamp(rng.next_normal_mean(1.7, 1.0), 0.5, 5.0),
            vg_l_per_kg: clamp(log_normal(&mut rng, -1.897, 0.23), 0.08, 0.30),
            vi_l_per_kg: clamp(rng.next_normal_mean(0.12, 0.012), 0.05, 0.25),
            ke_per_min: clamp(rng.next_normal_mean(0.14, 0.035), 0.02, 0.35),
            ka_per_min: clamp(rng.next_normal_mean(0.018, 0.0045), 0.002, 0.08),
            k12_per_min: premium.k12_per_min,
            sit_per_mu_l: premium.sit_per_mu_l,
            sid_per_mu_l: premium.sid_per_mu_l,
            sie_per_mu_l: premium.sie_per_mu_l,
            kb1_per_min: premium.kb1_per_min,
            kb2_per_min: premium.kb2_per_min,
            kb3_per_min: premium.kb3_per_min,
            f01_mmol_per_kg_min: premium.f01_mmol_per_kg_min,
            egp0_mmol_per_kg_min: premium.egp0_mmol_per_kg_min,
            r_thr_mmol_per_l: clamp(rng.next_normal_mean(9.0, 1.5), 5.0, 12.0),
            r_cl_per_min: clamp(rng.next_normal_mean(0.01, 0.025), 0.0, 0.3),
            bio_fraction: clamp(0.7 + rng.next_f64() * 0.5, 0.4, 1.5),
            t_max_g_min: clamp(log_normal(&mut rng, 3.689, 0.25), 20.0, 120.0),
            ug_ceil_mmol_per_kg_min: 0.02 + rng.next_f64() * 0.015,
            ka_int_per_min: clamp(log_normal(&mut rng, -2.372, 1.09), 0.02, 0.6),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn population_mean_is_reproducible() {
        assert_eq!(VirtualSubject::population_mean(), VirtualSubject::population_mean());
    }

    #[test]
    fn cohort_is_deterministic() {
        let a = cohort(4, 7);
        let b = cohort(4, 7);
        assert_eq!(a, b);
    }

    #[test]
    fn cohort_clamps_physiological() {
        for s in cohort(100, 1) {
            assert!(s.weight_kg >= 30.0 && s.weight_kg <= 150.0);
            assert!(s.bir_u_per_h > 0.0);
            assert!(s.t_max_g_min >= 20.0 && s.t_max_g_min <= 120.0);
            assert!(s.ug_ceil_mmol_per_kg_min >= 0.02 && s.ug_ceil_mmol_per_kg_min <= 0.035);
        }
    }

    #[test]
    fn basal_insulin_concentration_uses_basal_rate() {
        let s = VirtualSubject::population_mean();
        let bir_u_h = s.bir_u_per_h;
        let u_per_min = bir_u_h / 60.0 * 1000.0;
        let expected = u_per_min / (s.vi_l_per_kg * s.weight_kg * s.ke_per_min);
        assert!((s.basal_insulin_concentration() - expected).abs() < 1e-12);
    }
}