//! The virtual subject: Wilinska 2010's nominal parameters (Tables 1-2) and
//! its intra-subject oscillation.
//!
//! W10 publishes only the population means and priors, not its 18 subjects,
//! so there is one nominal subject, scaled to a body weight.

use tir_tuner_common::random::SeededRng;
use tir_tuner_common::settings::{DEFAULT_TARGET_MMOL_L, walsh_tdd_u};
use tir_tuner_common::units::{MIN_PER_HOUR, MU_PER_UNIT};

/// W10 population mean body weight (kg). src: w10-population
pub const W10_WEIGHT_KG: f64 = 74.9;

/// Carb factor `CarbF = 5.7 weight(kg) / TDD` (g/U). src: walsh-carb-factor
const WALSH_CARB_FACTOR_G_PER_KG: f64 = 5.7;
/// Grams per carb-ratio unit, "U per 10 g". src: walsh-carb-factor
pub const ICR_GRAMS: f64 = 10.0;

/// src: w10-table1
const F01_MMOL_PER_KG_MIN: f64 = 11.1e-3;
/// src: w10-table1
const EGP0_MMOL_PER_KG_MIN: f64 = 16.9e-3;
/// src: w10-table1
const K12_PER_MIN: f64 = 0.060;
/// src: w10-table1
const SIT_PER_MU_L: f64 = 18.41e-4;
/// src: w10-table1
const SID_PER_MU_L: f64 = 5.05e-4;
/// src: w10-table1
const SIE_PER_MU_L: f64 = 0.019;
/// src: w10-table1
const KB1_PER_MIN: f64 = 0.0034;
/// src: w10-table1
const KB2_PER_MIN: f64 = 0.056;
/// src: w10-table1
const KB3_PER_MIN: f64 = 0.024;

/// src: w10-table2
const VG_L_PER_KG: f64 = 0.15;
/// src: w10-table2
const VI_L_PER_KG: f64 = 0.12;
/// src: w10-table2
const KE_PER_MIN: f64 = 0.14;
/// src: w10-table2
const R_THR_MMOL_L: f64 = 9.0;
/// src: w10-table2
const R_CL_PER_MIN: f64 = 0.01;
/// src: w10-table2
const T_MAX_G_MIN: f64 = 40.0;
/// Median of W10's lognormal prior, exp(-2.372). src: w10-table2
const KA_INT_PER_MIN: f64 = 0.0933;
/// Mean of W10's U(70%, 120%). src: w10-bio
const BIO_FRACTION: f64 = 0.95;
/// Mean of W10's U(0.02, 0.035) mmol/kg/min. src: w10-ug-ceil
const UG_CEIL_MMOL_PER_KG_MIN: f64 = 0.0275;
/// Fiasp absorption rate (/min), fitted to Heise 2017 CSII. src: heise-fiasp-ka
const KA_FIASP_PER_MIN: f64 = 0.0207;

/// Parameters of a virtual subject.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VirtualSubject {
    /// Body weight (kg).
    pub weight_kg: f64,
    /// Glucose distribution volume (L/kg).
    pub vg_l_per_kg: f64,
    /// Insulin distribution volume (L/kg).
    pub vi_l_per_kg: f64,
    /// Insulin elimination rate (/min).
    pub ke_per_min: f64,
    /// Subcutaneous insulin absorption rate (/min).
    pub ka_per_min: f64,
    /// Transfer from the non-accessible to the accessible glucose mass (/min).
    pub k12_per_min: f64,
    /// Insulin sensitivity of glucose transport (/min per mU/L).
    pub sit_per_mu_l: f64,
    /// Insulin sensitivity of glucose disposal (/min per mU/L).
    pub sid_per_mu_l: f64,
    /// Insulin sensitivity of EGP suppression (per mU/L).
    pub sie_per_mu_l: f64,
    /// Rate of the insulin action on transport (/min).
    pub kb1_per_min: f64,
    /// Rate of the insulin action on disposal (/min).
    pub kb2_per_min: f64,
    /// Rate of the insulin action on EGP (/min).
    pub kb3_per_min: f64,
    /// Non-insulin-dependent glucose flux (mmol/kg/min).
    pub f01_mmol_per_kg_min: f64,
    /// EGP extrapolated to zero insulin (mmol/kg/min).
    pub egp0_mmol_per_kg_min: f64,
    /// Renal threshold (mmol/L).
    pub r_thr_mmol_per_l: f64,
    /// Renal clearance (/min).
    pub r_cl_per_min: f64,
    /// Carb bioavailability.
    pub bio_fraction: f64,
    /// Time-to-maximum of gut absorption (min).
    pub t_max_g_min: f64,
    /// Ceiling of the gut glucose flux (mmol/kg/min).
    pub ug_ceil_mmol_per_kg_min: f64,
    /// Plasma to interstitial glucose transfer (/min).
    pub ka_int_per_min: f64,
}

impl VirtualSubject {
    /// W10's nominal subject at `weight_kg`, with Fiasp absorption.
    pub fn nominal(weight_kg: f64) -> Self {
        Self {
            weight_kg,
            vg_l_per_kg: VG_L_PER_KG,
            vi_l_per_kg: VI_L_PER_KG,
            ke_per_min: KE_PER_MIN,
            ka_per_min: KA_FIASP_PER_MIN,
            k12_per_min: K12_PER_MIN,
            sit_per_mu_l: SIT_PER_MU_L,
            sid_per_mu_l: SID_PER_MU_L,
            sie_per_mu_l: SIE_PER_MU_L,
            kb1_per_min: KB1_PER_MIN,
            kb2_per_min: KB2_PER_MIN,
            kb3_per_min: KB3_PER_MIN,
            f01_mmol_per_kg_min: F01_MMOL_PER_KG_MIN,
            egp0_mmol_per_kg_min: EGP0_MMOL_PER_KG_MIN,
            r_thr_mmol_per_l: R_THR_MMOL_L,
            r_cl_per_min: R_CL_PER_MIN,
            bio_fraction: BIO_FRACTION,
            t_max_g_min: T_MAX_G_MIN,
            ug_ceil_mmol_per_kg_min: UG_CEIL_MMOL_PER_KG_MIN,
            ka_int_per_min: KA_INT_PER_MIN,
        }
    }

    /// Insulin flux (mU/min) that holds plasma insulin at `i_mu_per_l`.
    pub fn insulin_flux(&self, i_mu_per_l: f64) -> f64 {
        i_mu_per_l * self.vi_l_per_kg * self.weight_kg * self.ke_per_min
    }

    /// Basal (U/h) that holds glucose at rest at `g_mmol_per_l`.
    pub fn resting_basal_u_per_h(&self, g_mmol_per_l: f64) -> f64 {
        self.rate_for_insulin(crate::solver::resting_insulin(self, g_mmol_per_l))
    }

    /// Insulin-to-carb ratio (U per 10 g) for simulated meal boluses, Walsh
    /// 2011 eq 1-2 from the basal at the default target. W10's population
    /// 1.7 is a mean over subjects and overdoses the mean subject.
    pub fn walsh_icr(&self) -> f64 {
        ICR_GRAMS * self.walsh_tdd_u() / (WALSH_CARB_FACTOR_G_PER_KG * self.weight_kg)
    }

    /// The TDD (U) Walsh 2011 eq 1 gives for the basal at the default target.
    pub fn walsh_tdd_u(&self) -> f64 {
        walsh_tdd_u(self.resting_basal_u_per_h(DEFAULT_TARGET_MMOL_L))
    }

    /// The bolus (U) the Walsh ratio gives for `carbs_g`.
    pub fn meal_bolus_u(&self, carbs_g: f64) -> f64 {
        carbs_g / ICR_GRAMS * self.walsh_icr()
    }

    /// The infusion (U/h) that holds plasma insulin at `i_mu_per_l`.
    pub fn rate_for_insulin(&self, i_mu_per_l: f64) -> f64 {
        self.insulin_flux(i_mu_per_l) / MU_PER_UNIT * MIN_PER_HOUR
    }

    /// This subject at `t_min` with W10's oscillation applied.
    pub fn oscillated(&self, osc: &Oscillation, t_min: f64) -> Self {
        let [f01, egp0, k12, sid, sie, sit, kb1, kb2, kb3, ka, ke, ka_int] =
            osc.phase_min.map(|phase| {
                let angle = std::f64::consts::TAU * (t_min + phase) / OSCILLATION_PERIOD_MIN;
                1.0 + OSCILLATION_AMPLITUDE * angle.sin()
            });
        Self {
            f01_mmol_per_kg_min: self.f01_mmol_per_kg_min * f01,
            egp0_mmol_per_kg_min: self.egp0_mmol_per_kg_min * egp0,
            k12_per_min: self.k12_per_min * k12,
            sid_per_mu_l: self.sid_per_mu_l * sid,
            sie_per_mu_l: self.sie_per_mu_l * sie,
            sit_per_mu_l: self.sit_per_mu_l * sit,
            kb1_per_min: self.kb1_per_min * kb1,
            kb2_per_min: self.kb2_per_min * kb2,
            kb3_per_min: self.kb3_per_min * kb3,
            ka_per_min: self.ka_per_min * ka,
            ke_per_min: self.ke_per_min * ke,
            ka_int_per_min: self.ka_int_per_min * ka_int,
            ..*self
        }
    }
}

/// Amplitude of the sinusoidal parameter oscillation. src: w10-oscillation
const OSCILLATION_AMPLITUDE: f64 = 0.05;
/// Period of the oscillation (min). src: w10-oscillation
const OSCILLATION_PERIOD_MIN: f64 = 180.0;
/// Parameters W10 Tables 1-2 mark oscillatory. src: w10-oscillation
pub const OSCILLATING_PARAMETERS: usize = 12;

/// Per-parameter phases of W10's oscillation, each from U[0, 3 h].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oscillation {
    /// Phase (min) per oscillating parameter, in [`VirtualSubject::oscillated`]'s order.
    pub phase_min: [f64; OSCILLATING_PARAMETERS],
}

impl Oscillation {
    /// Phases drawn from U[0, 3 h].
    pub fn draw(rng: &mut SeededRng) -> Self {
        Self {
            phase_min: std::array::from_fn(|_| rng.next_f64() * OSCILLATION_PERIOD_MIN),
        }
    }
}

/// Interoccasion CV of the meal bioavailability. W10 gives the 20%, not the
/// distribution. src: w10-bio-interoccasion
const BIO_INTEROCCASION_CV: f64 = 0.2;

/// One meal's bioavailability factor around 1, floored at 0.
pub fn meal_bio_factor(rng: &mut SeededRng) -> f64 {
    rng.next_normal_mean(1.0, BIO_INTEROCCASION_CV).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walsh_icr_from_the_resting_basal() {
        let s = VirtualSubject::nominal(50.0);
        let carb_factor = 5.7 * 50.0 / (s.resting_basal_u_per_h(5.8) * 24.0 / 0.48);
        assert!((s.walsh_icr() - 10.0 / carb_factor).abs() < 1e-12);
        assert!((s.meal_bolus_u(60.0) - 6.0 * s.walsh_icr()).abs() < 1e-12);
        // Per-kg model: the ratio doesn't depend on weight.
        let w10 = VirtualSubject::nominal(W10_WEIGHT_KG).walsh_icr();
        assert!((w10 - s.walsh_icr()).abs() < 1e-9, "{w10}");
    }

    #[test]
    fn rate_for_insulin_is_the_clearance() {
        let s = VirtualSubject::nominal(50.0);
        // 10 mU/L x 0.12 L/kg x 50 kg x 0.14 /min, in U/h.
        assert!((s.rate_for_insulin(10.0) - 10.0 * 0.84 * 60.0 / 1000.0).abs() < 1e-12);
    }

    #[test]
    fn w10_oscillation_is_5_percent_over_3_hours() {
        let s = VirtualSubject::nominal(W10_WEIGHT_KG);
        let osc = Oscillation {
            phase_min: [0.0; OSCILLATING_PARAMETERS],
        };
        // Peak a quarter period in, back to nominal after a full one.
        assert!((s.oscillated(&osc, 45.0).sie_per_mu_l - 0.019 * 1.05).abs() < 1e-12);
        assert!((s.oscillated(&osc, 180.0).ka_int_per_min - 0.0933).abs() < 1e-12);
        // Stationary parameters don't move.
        assert_eq!(s.oscillated(&osc, 45.0).vg_l_per_kg, 0.15);
    }

    #[test]
    fn each_oscillating_parameter_has_its_own_phase() {
        let s = VirtualSubject::nominal(W10_WEIGHT_KG);
        let ratios = |o: VirtualSubject| {
            [
                o.f01_mmol_per_kg_min / s.f01_mmol_per_kg_min,
                o.egp0_mmol_per_kg_min / s.egp0_mmol_per_kg_min,
                o.k12_per_min / s.k12_per_min,
                o.sid_per_mu_l / s.sid_per_mu_l,
                o.sie_per_mu_l / s.sie_per_mu_l,
                o.sit_per_mu_l / s.sit_per_mu_l,
                o.kb1_per_min / s.kb1_per_min,
                o.kb2_per_min / s.kb2_per_min,
                o.kb3_per_min / s.kb3_per_min,
                o.ka_per_min / s.ka_per_min,
                o.ke_per_min / s.ke_per_min,
                o.ka_int_per_min / s.ka_int_per_min,
            ]
        };
        for k in 0..OSCILLATING_PARAMETERS {
            let mut phase_min = [0.0; OSCILLATING_PARAMETERS];
            phase_min[k] = 45.0;
            let r = ratios(s.oscillated(&Oscillation { phase_min }, 0.0));
            for (j, v) in r.iter().enumerate() {
                let want = if j == k { 1.05 } else { 1.0 };
                assert!((v - want).abs() < 1e-12, "phase {k}, parameter {j}: {v}");
            }
        }
    }

    #[test]
    fn w10_phases_are_uniform_over_3_hours() {
        let mut rng = SeededRng::new(5);
        let phases: Vec<f64> = (0..500)
            .flat_map(|_| Oscillation::draw(&mut rng).phase_min)
            .collect();
        assert!(phases.iter().all(|p| (0.0..180.0).contains(p)));
        let mean = phases.iter().sum::<f64>() / phases.len() as f64;
        assert!((mean - 90.0).abs() < 2.0, "mean {mean}");
    }

    #[test]
    fn w10_meal_bioavailability_varies_20_percent() {
        let mut rng = SeededRng::new(6);
        let f: Vec<f64> = (0..20_000).map(|_| meal_bio_factor(&mut rng)).collect();
        let mean = f.iter().sum::<f64>() / f.len() as f64;
        let sd = (f.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / f.len() as f64).sqrt();
        assert!((mean - 1.0).abs() < 0.01, "mean {mean}");
        assert!((sd - 0.2).abs() < 0.01, "sd {sd}");
    }
}
