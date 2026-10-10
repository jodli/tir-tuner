//! The controller's glucoregulatory model: the patent's equations
//! (US8977504B2 eq 1-16), stepped with forward Euler and every mass clamped
//! at zero.
//!
//! Units: insulin masses `i1`, `i2` in mU, concentrations in mU/L, carbs
//! `a1`, `a2` in g, glucose masses in mmol/kg. Inputs: basal U/h, bolus U,
//! meal g/min.

use tir_tuner_common::euler::{bisect_falling, clamped_forward_euler};
use tir_tuner_common::settings::walsh_basal_u_per_h;
use tir_tuner_common::units::{MIN_PER_HOUR, MMOL_PER_GRAM_CHO, MU_PER_UNIT};

/// Halftime (min) at which `u_S` regresses to zero without a measurement,
/// eq 57. The patent gives no value. src: own-us-halftime
pub const U_S_HALFTIME_MIN: f64 = 15.0;

/// Plasma glucose that basal insulin `BIC` holds (mmol/L), eq 12.
/// src: us504-bic-rest
pub const REST_MMOL_L: f64 = 5.5;

/// src: h04-table1
const H04_K12_PER_MIN: f64 = 0.066;
/// src: h04-table1
const H04_KA2_PER_MIN: f64 = 0.06;
/// src: h04-table1
const H04_KA3_PER_MIN: f64 = 0.03;
/// src: h04-table1
const H04_KE_PER_MIN: f64 = 0.138;
/// src: h04-table1
const H04_VI_L_PER_KG: f64 = 0.12;
/// src: h04-table1
const H04_VG_L_PER_KG: f64 = 0.16;
/// src: h04-table1
const H04_BIOAVAILABILITY: f64 = 0.8;
/// src: h04-table1
const H04_T_MAX_G_MIN: f64 = 40.0;
/// src: h04-table2
const H04_SIT_PER_MU_L: f64 = 51.2e-4;
/// src: h04-table2
const H04_SID_PER_MU_L: f64 = 8.2e-4;
/// src: h04-table2
const H04_SIE_PER_MU_L: f64 = 520e-4;
/// src: h04-table2
const H04_EGP0_MMOL_PER_KG_MIN: f64 = 0.0161;
/// src: h04-table2
const H04_F01_MMOL_PER_KG_MIN: f64 = 0.0097;
/// Lispro; H04 has no Fiasp value. src: h04-table2
const H04_T_MAX_I_MIN: f64 = 55.0;
/// Plasma-interstitial transfer, W10's median; H04 samples plasma.
/// src: w10-table2
const W10_K31_PER_MIN: f64 = 0.0933;

/// Basal plasma insulin `BIC` (mU/L), US8977504B2 eq 14.
pub fn basal_insulin_conc(bir_u_per_h: f64, mcr_i: f64, weight_kg: f64) -> f64 {
    MU_PER_UNIT * bir_u_per_h / (MIN_PER_HOUR * mcr_i * weight_kg)
}

/// EGP (mmol/kg/min) at remote action `r_e`: halves for every `i_half`
/// mU/L above `bic`. US8977504B2 eq 12. src: us504-egp
pub fn egp(r_e: f64, bic: f64, egp_b: f64, i_half: f64) -> f64 {
    egp_b * (-(r_e - bic) * std::f64::consts::LN_2 / i_half).exp()
}

/// Model parameters. The patent names them without values.
#[derive(Clone, Copy, Debug)]
pub struct HovorkaParams {
    /// Time-to-peak of subcutaneous insulin absorption (min).
    pub t_max_i: f64,
    /// Time-to-peak of gut absorption (min).
    pub t_max_g: f64,
    /// Metabolic clearance rate of insulin (L/kg/min).
    pub mcr_i: f64,
    /// Body weight (kg).
    pub weight_kg: f64,
    /// Fractional disappearance rate of the remote action on disposal (/min).
    pub p2_d: f64,
    /// Fractional disappearance rate of the remote action on EGP (/min).
    pub p2_e: f64,
    /// Transfer q2 -> q1 (/min).
    pub k12: f64,
    /// Transfer q1 -> q2 (/min).
    pub k21: f64,
    /// Transfer between plasma and interstitial glucose (/min).
    pub k31: f64,
    /// Glucose distribution volume (L/kg).
    pub v_g: f64,
    /// Non-insulin-dependent glucose flux (mmol/kg/min), constant as in
    /// patent eq 9.
    pub f_01: f64,
    /// Insulin sensitivity of disposal (/min per mU/L).
    pub s_id: f64,
    /// Basal EGP (mmol/kg/min), at plasma insulin `BIC`.
    pub egp_b: f64,
    /// Plasma insulin increment that halves EGP (mU/L), eq 12's `I_1/2`.
    pub i_half: f64,
    /// Carb bioavailability `F` (patent eq 9).
    pub bioavailability: f64,
    /// Basal insulin requirement (U/h), which sets `BIC`.
    pub bir: f64,
}

impl HovorkaParams {
    /// Basal plasma insulin concentration `BIC` (mU/L).
    pub fn basal_insulin_conc(&self) -> f64 {
        basal_insulin_conc(self.bir, self.mcr_i, self.weight_kg)
    }

    /// `s_id` solved so basal insulin holds plasma glucose at `g` mmol/L.
    #[cfg(test)]
    pub fn resting_at(self, g: f64) -> Self {
        let surplus = self.egp_b - self.f_01;
        Self {
            s_id: surplus / (g * self.v_g * self.basal_insulin_conc()),
            ..self
        }
    }

    /// EGP at remote action `r_e` (mmol/kg/min).
    pub fn egp(&self, r_e: f64) -> f64 {
        egp(r_e, self.basal_insulin_conc(), self.egp_b, self.i_half)
    }

    /// The model from weight and TDD (manual p29, p70): H04 scaled to rest
    /// at 5.5 mmol/L on `BIC`, linearized at `BIC`. src: own-tdd-sensitivity
    pub fn from_weight_and_tdd(weight_kg: f64, tdd_u: f64) -> Self {
        let bir = walsh_basal_u_per_h(tdd_u);
        let mcr_i = H04_VI_L_PER_KG * H04_KE_PER_MIN;
        let bic = basal_insulin_conc(bir, mcr_i, weight_kg);
        let q1 = REST_MMOL_L * H04_VG_L_PER_KG;
        let (sit, sid, sie) = h04_sensitivities(h04_sensitivity_scale(bic, q1));
        let egp_b = H04_EGP0_MMOL_PER_KG_MIN * (1.0 - sie * bic);
        // d/dI of H04's uptake x2 x1 Q1 / (x2 + k12), per unit of q1.
        let d = sid * bic + H04_K12_PER_MIN;
        let s_id = sit * sid * bic * (d + H04_K12_PER_MIN) / (d * d);
        Self {
            t_max_i: H04_T_MAX_I_MIN,
            t_max_g: H04_T_MAX_G_MIN,
            mcr_i,
            weight_kg,
            p2_d: H04_KA2_PER_MIN,
            p2_e: H04_KA3_PER_MIN,
            k12: H04_K12_PER_MIN,
            k21: sit * bic,
            k31: W10_K31_PER_MIN,
            v_g: H04_VG_L_PER_KG,
            f_01: egp_b - s_id * bic * q1,
            s_id,
            egp_b,
            i_half: std::f64::consts::LN_2 * (1.0 / sie - bic),
            bioavailability: H04_BIOAVAILABILITY,
            bir,
        }
    }
}

/// H04's SIT, SID, SIE scaled by `s`.
fn h04_sensitivities(s: f64) -> (f64, f64, f64) {
    (
        s * H04_SIT_PER_MU_L,
        s * H04_SID_PER_MU_L,
        s * H04_SIE_PER_MU_L,
    )
}

/// H04's net glucose flux at rest (eq 1-4 at 5.5 mmol/L: F01c = F01, no
/// renal loss) with sensitivities scaled by `s`.
fn h04_rest_flux(s: f64, bic: f64, q1: f64) -> f64 {
    let (sit, sid, sie) = h04_sensitivities(s);
    let (x1, x2) = (sit * bic, sid * bic);
    H04_EGP0_MMOL_PER_KG_MIN * (1.0 - sie * bic)
        - H04_F01_MMOL_PER_KG_MIN
        - x2 * x1 * q1 / (H04_K12_PER_MIN + x2)
}

/// The scale at which H04 rests at `q1 / VG` on `bic`; the flux falls with
/// it and EGP is fully suppressed at the top of the bracket.
fn h04_sensitivity_scale(bic: f64, q1: f64) -> f64 {
    bisect_falling(
        |s| h04_rest_flux(s, bic, q1),
        0.0,
        1.0 / (H04_SIE_PER_MU_L * bic),
    )
}

/// Model state, patent eq 1-16.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HovorkaState {
    /// Subcutaneous insulin, depot 1 (mU).
    pub i1: f64,
    /// Subcutaneous insulin, depot 2 (mU).
    pub i2: f64,
    /// Remote insulin action on glucose disposal (mU/L).
    pub r_d: f64,
    /// Remote insulin action on EGP (mU/L).
    pub r_e: f64,
    /// Carbs in gut compartment 1 (g).
    pub a1: f64,
    /// Carbs in gut compartment 2 (g).
    pub a2: f64,
    /// Glucose in the accessible compartment (mmol/kg).
    pub q1: f64,
    /// Glucose in the non-accessible compartment (mmol/kg).
    pub q2: f64,
    /// Interstitial glucose (mmol/kg), what the CGM reads.
    pub q3: f64,
    /// Unexplained glucose influx `u_S` (mmol/kg/min), patent eq 13,
    /// estimated by [`crate::kalman`]. Held by [`HovorkaState::step`].
    pub u_s: f64,
}

impl HovorkaState {
    /// Insulin at its basal steady state, no carbs, glucose at `g` mmol/L.
    /// Glucose as the patent's initial state, eq 63.
    pub fn basal(params: &HovorkaParams, g: f64) -> Self {
        let depot = MU_PER_UNIT * params.bir / MIN_PER_HOUR * params.t_max_i;
        let bic = params.basal_insulin_conc();
        let mut s = Self {
            i1: depot,
            i2: depot,
            r_d: bic,
            r_e: bic,
            a1: 0.0,
            a2: 0.0,
            q1: 0.0,
            q2: 0.0,
            q3: 0.0,
            u_s: 0.0,
        };
        s.shift_glucose(params, g);
        s
    }

    /// Add `dg` mmol/L to plasma and interstitial glucose, and the matching
    /// amount to the non-accessible compartment (eq 63's ratio).
    pub fn shift_glucose(&mut self, params: &HovorkaParams, dg: f64) {
        let dq = dg * params.v_g;
        self.q1 += dq;
        self.q2 += dq * params.k21 / params.k12;
        self.q3 += dq;
    }

    /// Plasma insulin `i` (mU/L), patent eq 3.
    pub fn insulin_conc(&self, params: &HovorkaParams) -> f64 {
        self.i2 / (params.t_max_i * params.mcr_i * params.weight_kg)
    }

    /// Gut absorption `u_A` (mmol/kg/min), patent eq 8: grams to mmol.
    pub fn gut_absorption(&self, params: &HovorkaParams) -> f64 {
        MMOL_PER_GRAM_CHO * self.a2 / (params.t_max_g * params.weight_kg)
    }

    /// Right-hand side of patent eq 1-16 for an insulin rate (U/h; a bolus
    /// is a rate over its delivery time) and a meal rate. `u_s` doesn't
    /// change.
    pub fn derivative(
        &self,
        params: &HovorkaParams,
        u_basal: f64,
        meal_g_per_min: f64,
    ) -> HovorkaState {
        let i = self.insulin_conc(params);
        let insulin_mu_per_min = MU_PER_UNIT * u_basal / MIN_PER_HOUR;
        HovorkaState {
            i1: insulin_mu_per_min - self.i1 / params.t_max_i,
            i2: (self.i1 - self.i2) / params.t_max_i,
            r_d: params.p2_d * (i - self.r_d),
            r_e: params.p2_e * (i - self.r_e),
            a1: meal_g_per_min - self.a1 / params.t_max_g,
            a2: (self.a1 - self.a2) / params.t_max_g,
            q1: -(params.s_id * self.r_d + params.k21) * self.q1 + params.k12 * self.q2
                - params.f_01
                + params.egp(self.r_e)
                + params.bioavailability * self.gut_absorption(params)
                + self.u_s,
            q2: params.k21 * self.q1 - params.k12 * self.q2,
            q3: params.k31 * (self.q1 - self.q3),
            u_s: 0.0,
        }
    }

    /// One forward Euler step of `dt` minutes, every mass clamped at zero.
    pub fn step(
        &self,
        params: &HovorkaParams,
        u_basal: f64,
        meal_g_per_min: f64,
        dt: f64,
    ) -> HovorkaState {
        let d = self.derivative(params, u_basal, meal_g_per_min);
        let e = |x, dx| clamped_forward_euler(x, dx, dt);
        HovorkaState {
            i1: e(self.i1, d.i1),
            i2: e(self.i2, d.i2),
            r_d: e(self.r_d, d.r_d),
            r_e: e(self.r_e, d.r_e),
            a1: e(self.a1, d.a1),
            a2: e(self.a2, d.a2),
            q1: e(self.q1, d.q1),
            q2: e(self.q2, d.q2),
            q3: e(self.q3, d.q3),
            u_s: self.u_s,
        }
    }

    /// [`HovorkaState::step`] without a measurement at its end: `u_S`
    /// regresses toward zero with [`U_S_HALFTIME_MIN`] (eq 57).
    pub fn step_unmeasured(
        &self,
        params: &HovorkaParams,
        u_basal: f64,
        meal_g_per_min: f64,
        dt: f64,
    ) -> HovorkaState {
        let mut s = self.step(params, u_basal, meal_g_per_min, dt);
        s.u_s *= (-std::f64::consts::LN_2 * dt / U_S_HALFTIME_MIN).exp();
        s
    }

    /// Plasma glucose (mmol/L), patent eq 11.
    pub fn plasma_glucose(&self, params: &HovorkaParams) -> f64 {
        self.q1 / params.v_g
    }

    /// Interstitial glucose (mmol/L), patent eq 16.
    pub fn interstitial_glucose(&self, params: &HovorkaParams) -> f64 {
        self.q3 / params.v_g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A basal other than 1 U/h, so scaling by it shows.
    fn params() -> HovorkaParams {
        HovorkaParams::from_weight_and_tdd(70.0, 40.0)
    }

    /// Patent eq 8: u_A = 5.551 a2 / (W t_max,G).
    #[test]
    fn us504_gut_absorption_is_grams_times_5_551() {
        let p = params();
        let s = HovorkaState {
            a2: 50.0,
            ..HovorkaState::basal(&p, 6.0)
        };
        let expected = 5.551 * 50.0 / (p.weight_kg * p.t_max_g);
        assert!((s.gut_absorption(&p) - expected).abs() < 1e-4);
    }

    /// Eq 12: basal insulin holds 5.5 mmol/L, in the patent's model and in
    /// the scaled H04 model it came from.
    #[test]
    fn us504_weight_and_tdd_model_rests_at_5_5() {
        let p = HovorkaParams::from_weight_and_tdd(50.0, 36.0);
        let s = HovorkaState::basal(&p, 5.5);
        assert!(s.derivative(&p, p.bir, 0.0).q1.abs() < 1e-15);
        let bic = p.basal_insulin_conc();
        let scale = h04_sensitivity_scale(bic, 5.5 * 0.16);
        // 36 U/day at 50 kg: BIC 14.5 mU/L, H04's sensitivities at 39%.
        assert!((bic - 14.4928).abs() < 1e-4, "{bic}");
        assert!((scale - 0.38985).abs() < 1e-5, "{scale}");
        // Values from an independent secant solve of H04's rest balance.
        assert!((p.egp_b - 0.0113698).abs() < 1e-7, "{}", p.egp_b);
        assert!((p.k21 - 0.0289282).abs() < 1e-7, "{}", p.k21);
        assert!((p.i_half - 24.1462).abs() < 1e-4, "{}", p.i_half);
        // Uptake slope matches H04's at BIC, by central difference.
        let h04_uptake = |i: f64| {
            let (x1, x2) = (scale * 51.2e-4 * i, scale * 8.2e-4 * i);
            x2 * x1 * 5.5 * 0.16 / (x2 + 0.066)
        };
        let h = 1e-3;
        let slope = (h04_uptake(bic + h) - h04_uptake(bic - h)) / (2.0 * h);
        assert!(
            (p.s_id * 5.5 * 0.16 - slope).abs() < 1e-10,
            "{} vs {slope}",
            p.s_id
        );
        // The manual's extreme (5 U/day at 300 kg): BIC below 1 mU/L, and the
        // scale far above 1, still inside the bracket.
        let p = HovorkaParams::from_weight_and_tdd(300.0, 5.0);
        let scale = h04_sensitivity_scale(p.basal_insulin_conc(), 5.5 * 0.16);
        assert!((scale - 16.8416).abs() < 1e-4, "{scale}");
        assert!((p.i_half - 0.558939).abs() < 1e-6, "{}", p.i_half);
    }

    /// Eq 57: without measurements `u_S` halves every halftime.
    #[test]
    fn us504_u_s_regresses_with_its_halftime() {
        let p = params();
        let mut s = HovorkaState {
            u_s: 0.01,
            ..HovorkaState::basal(&p, 6.0)
        };
        // Three steps, so dt itself counts, not just their number.
        let dt = U_S_HALFTIME_MIN / 3.0;
        for _ in 0..3 {
            s = s.step_unmeasured(&p, p.bir, 0.0, dt);
        }
        assert!((s.u_s - 0.005).abs() < 1e-12);
    }

    /// Patent eq 12: EGP_B at BIC, half of it one I_1/2 higher.
    #[test]
    fn us504_egp_halves_per_i_half() {
        let p = params();
        let bic = p.basal_insulin_conc();
        assert_eq!(p.egp(bic), p.egp_b);
        assert!((p.egp(bic + p.i_half) - p.egp_b / 2.0).abs() < 1e-12);
    }

    /// Patent eq 6, 7, 9 (u_S term) and 15.
    #[test]
    fn us504_gut_and_interstitial_terms() {
        let p = params();
        let s = HovorkaState {
            a1: 40.0,
            a2: 10.0,
            q3: 0.5,
            ..HovorkaState::basal(&p, 6.0)
        };
        let d = s.derivative(&p, p.bir, 2.0);
        assert!((d.a1 - (2.0 - 40.0 / p.t_max_g)).abs() < 1e-12);
        assert!((d.a2 - 30.0 / p.t_max_g).abs() < 1e-12);
        assert!((d.q3 - p.k31 * (s.q1 - 0.5)).abs() < 1e-12);
        let with_u_s = HovorkaState { u_s: 0.01, ..s }.derivative(&p, p.bir, 2.0);
        assert!((with_u_s.q1 - d.q1 - 0.01).abs() < 1e-12);
    }

    /// Patent eq 3 and 14 agree: a basal of BIR U/h holds plasma insulin
    /// and both remote actions at BIC.
    #[test]
    fn us504_basal_insulin_holds_bic() {
        let p = params();
        let s = HovorkaState::basal(&p, 6.0);
        assert!((s.insulin_conc(&p) - p.basal_insulin_conc()).abs() < 1e-9);
        let d = s.derivative(&p, p.bir, 0.0);
        for v in [d.i1, d.i2, d.r_d, d.r_e] {
            assert!(v.abs() < 1e-9);
        }
    }

    #[cfg(test)]
    mod prop_tests {
        use super::*;
        use crate::test_support::config;
        use proptest::prelude::*;

        fn state_strategy() -> impl Strategy<Value = HovorkaState> {
            (
                0.0..25_000.0f64,
                0.0..25_000.0f64,
                0.0..50.0f64,
                0.0..50.0f64,
                0.0..150.0f64,
                0.0..150.0f64,
                0.0..30.0f64,
                0.0..30.0f64,
                0.0..30.0f64,
                0.0..5.0f64,
            )
                .prop_map(|(i1, i2, r_d, r_e, a1, a2, q1, q2, q3, u_s)| HovorkaState {
                    i1,
                    i2,
                    r_d,
                    r_e,
                    a1,
                    a2,
                    q1,
                    q2,
                    q3,
                    u_s,
                })
        }

        fn assert_finite_nonnegative(
            state: &HovorkaState,
        ) -> Result<(), proptest::test_runner::TestCaseError> {
            for (name, value) in [
                ("i1", state.i1),
                ("i2", state.i2),
                ("r_d", state.r_d),
                ("r_e", state.r_e),
                ("a1", state.a1),
                ("a2", state.a2),
                ("q1", state.q1),
                ("q2", state.q2),
                ("q3", state.q3),
            ] {
                prop_assert!(value.is_finite(), "{name} is not finite: {value}");
                prop_assert!(value >= 0.0, "{name} became negative: {value}");
            }
            Ok(())
        }

        proptest! {
            #![proptest_config(config())]

            #[test]
            fn random_walk_stays_finite_and_nonnegative(
                start in state_strategy(),
                inputs in prop::collection::vec(
                    (0.0..25.0f64, 0.0..5.0f64, 0.5..3.0f64),
                    0..64,
                ),
            ) {
                let params = super::params();
                let mut state = start;
                for (u_basal, meal, dt) in inputs {
                    state = state.step(&params, u_basal, meal, dt);
                    assert_finite_nonnegative(&state)?;
                }
            }
        }
    }
}
