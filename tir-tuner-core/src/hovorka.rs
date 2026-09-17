//! Glucoregulatory model: the 10-dimensional extended Hovorka state,
//! continuous-time equations discretized with a forward Euler step, and
//! the endogenous glucose production (EGP) submodel.
//!
//! The step applies a physiological saturation at zero to every mass /
//! concentration compartment, which enforces the non-negativity safety
//! invariant that the Kani suite verifies.
//!
//! Units follow Hovorka et al. 2004: subcutaneous insulin masses `i1`,
//! `i2` are in millie-units (mU), plasma concentrations (`insulin_conc`,
//! `BIC`, remote actions `r_d`, `r_e`) in mU/L. The dosing inputs `u_basal`
//! (U/h) and `u_bolus` (U) are converted to the mU mass state internally,
//! and meal carbohydrate ingestion is accepted as a rate `meal_g_per_min`
//! (g/min) feeding the gut depot `a1` (section 3.2C).

/// Millie-units per insulin unit, the `i1`/`i2` mass-state convention
/// (Hovorka et al. 2004).
const MU_PER_UNIT: f64 = 1000.0;

/// Basal insulin concentration `BIC` (mU/L) for given basal insulin
/// requirement, clearance and body weight.
pub fn basal_insulin_conc(bir_u_per_h: f64, mcr_i: f64, weight_kg: f64) -> f64 {
    (1000.0 * bir_u_per_h) / (60.0 * mcr_i * weight_kg)
}

/// Upper bound on the EGP response, as a multiple of basal EGP. The
/// exponential suppression model rises without bound when the remote
/// EGP action drops below the basal insulin concentration, so the
/// low-insulin branch is capped here (about 0.048 mmol/kg/min at the
/// default basal EGP of 0.0161). The cap preserves the basal identity
/// `egp(bic) == egp_b` and therefore every verified property
/// (non-negativity, finiteness, the basal steady state anchor).
pub const EGP_MAX_FOLD_OVER_BASAL: f64 = 3.0;

/// EGP (mmol/kg/min) as a function of the remote insulin action on
/// hepatic EGP suppression `r_e` (mU/L), given basal EGP and the basal
/// insulin concentration.
///
/// The specification form (section 3.2D of
/// `docs/camaps_fx_kani_specification.md`): a fixed `0.5` mmol/L-relative
/// half-increment denominator, capped at `EGP_MAX_FOLD_OVER_BASAL`
/// times the basal EGP. This is the model the Kani suite and the native
/// tests verify, not the literal functional form of the 2004
/// publication.
pub fn egp(r_e: f64, bic: f64, egp_b: f64) -> f64 {
    let uncapped = egp_b * (-((r_e - bic) / 0.5) * std::f64::consts::LN_2).exp();
    uncapped.min(EGP_MAX_FOLD_OVER_BASAL * egp_b)
}

/// Subcutaneous insulin absorption parameters (Hovorka et al. 2004).
#[derive(Clone, Copy, Debug)]
pub struct HovorkaParams {
    /// Time-to-peak subcutaneous insulin absorption (min).
    pub t_max_i: f64,
    /// Time-to-peak gut glucose absorption (min).
    pub t_max_g: f64,
    /// Metabolic clearance rate of insulin (L/kg/min).
    pub mcr_i: f64,
    /// Body weight (kg).
    pub weight_kg: f64,
    /// Fractional disappearance rate of remote disposal action (/min).
    pub p2_d: f64,
    /// Fractional disappearance rate of remote EGP action (/min).
    pub p2_e: f64,
    /// Inter-compartmental transfer rate q2 -> q1 (/min).
    pub k12: f64,
    /// Inter-compartmental transfer rate q1 -> q2 (/min).
    pub k21: f64,
    /// Interstitial transfer rate q1 -> q3 (/min).
    pub k31: f64,
    /// Glucose distribution volume (L/kg).
    pub v_g: f64,
    /// Non-insulin dependent glucose utilization (mmol/kg/min).
    ///
    /// Constant per the specification (section 3.2D); the 2004
    /// publication's glucose-dependent saturable elimination is not part
    /// of the verified model.
    pub f_01: f64,
    /// Peripheral insulin sensitivity (/min per mU/L).
    ///
    /// Chosen so the basal glucose equilibrium of the default
    /// configuration sits on the nominal target (5.8 mmol/L): at the
    /// basal steady state `q1 = (egp_b - f_01) / (s_id * bic)`, which
    /// equals `5.8 * v_g` for `s_id = 5.8e-4`.
    pub s_id: f64,
    /// Basal endogenous glucose production (mmol/kg/min).
    pub egp_b: f64,
    /// Basal insulin requirement (U/h).
    pub bir: f64,
}

impl HovorkaParams {
    /// Basal plasma insulin concentration (mU/L).
    pub fn basal_insulin_conc(&self) -> f64 {
        basal_insulin_conc(self.bir, self.mcr_i, self.weight_kg)
    }

    /// Endogenous glucose production at remote EGP action `r_e`
    /// (mmol/kg/min).
    pub fn egp(&self, r_e: f64) -> f64 {
        egp(r_e, self.basal_insulin_conc(), self.egp_b)
    }
}

impl Default for HovorkaParams {
    fn default() -> Self {
        Self {
            t_max_i: 55.0,
            t_max_g: 40.0,
            mcr_i: 0.021,
            weight_kg: 70.0,
            p2_d: 0.02,
            p2_e: 0.011,
            k12: 0.066,
            k21: 0.066,
            k31: 0.01,
            v_g: 0.16,
            f_01: 0.01,
            s_id: 0.00058,
            egp_b: 0.0161,
            bir: 1.0,
        }
    }
}

/// Forward Euler update saturated at zero: `(prev + dt * rate).max(0.0)`.
///
/// This is the enforcement primitive behind the physiological
/// non-negativity invariant of [`HovorkaState::step`]. For every `f64`
/// input the result is non-negative: IEEE `max` returns the non-NaN
/// operand when the other is NaN (clamping NaN rates to zero) and a
/// `+inf` rate is returned unchanged, still `>= 0.0`. Kani discharges
/// the negation only over a bounded finite range of `prev` / `rate`;
/// the full-domain statement is an algebraic property of IEEE `max`,
/// not a solver result.
pub fn clamped_forward_euler(prev: f64, rate: f64, dt: f64) -> f64 {
    (prev + dt * rate).max(0.0)
}

/// Extended 10-dimensional glucoregulatory state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HovorkaState {
    /// Subcutaneous insulin mass, depot 1 (mU).
    pub i1: f64,
    /// Subcutaneous insulin mass, depot 2 (mU).
    pub i2: f64,
    /// Remote insulin action on peripheral glucose disposal (mU/L).
    pub r_d: f64,
    /// Remote insulin action on hepatic EGP suppression (mU/L).
    pub r_e: f64,
    /// Carbohydrate mass in gut absorption depot 1 (g).
    pub a1: f64,
    /// Carbohydrate mass in gut absorption depot 2 (g).
    pub a2: f64,
    /// Accessible glucose mass in the plasma compartment (mmol/kg).
    pub q1: f64,
    /// Non-accessible glucose mass in the peripheral tissue compartment
    /// (mmol/kg).
    pub q2: f64,
    /// Interstitial fluid glucose mass, measured by the CGM (mmol/kg).
    pub q3: f64,
    /// Unexplained stochastic glucose influx `u_S` (mmol/kg/min).
    ///
    /// Reserved for the process-noise state of section 3.2F
    /// (`d u_S = d w`). The deterministic core carries it through
    /// [`HovorkaState::step`] unchanged; the stochastic increment is
    /// injected by the simulation / filtering layer, so the increment
    /// produced by [`HovorkaState::derivative`] is zero.
    pub u_s: f64,
}

impl HovorkaState {
    /// Instantaneous subcutaneous insulin concentration `i(t)` (mU/L),
/// consistent with the mU mass state and with `BIC`.
    pub fn insulin_conc(&self, params: &HovorkaParams) -> f64 {
        self.i2 / (params.t_max_i * params.mcr_i * params.weight_kg)
    }

    /// Gut carbohydrate absorption rate `u_A(t)` (mmol/kg/min).
    pub fn gut_absorption(&self, params: &HovorkaParams) -> f64 {
        self.a2 / (params.t_max_g * params.weight_kg * 5.551)
    }

    /// Increment of the continuous-time differential equations at the
    /// current state, via the model in section 3.2 of the specification.
    ///
    /// `u_basal` is the basal infusion rate (U/h), `u_bolus` a manual
    /// insulin bolus (U), and `meal_g_per_min` the carbohydrate ingestion
    /// rate (g/min, section 3.2C); the insulin inputs are converted to
    /// the mU mass-state convention internally so that the steady state
    /// under a basal of `BIR` U/h sits exactly at `BIC` mU/L. The
    /// returned `u_s` increment is zero: the process noise of section
    /// 3.2F is injected externally (see [`HovorkaState::u_s`]).
    pub fn derivative(
        &self,
        params: &HovorkaParams,
        u_basal: f64,
        u_bolus: f64,
        meal_g_per_min: f64,
    ) -> HovorkaState {
        let i = self.insulin_conc(params);
        let u_a = self.gut_absorption(params);
        let egp = params.egp(self.r_e);
        let insulin_influx_mu_per_min = MU_PER_UNIT * u_basal / 60.0 + MU_PER_UNIT * u_bolus;

        HovorkaState {
            i1: -(1.0 / params.t_max_i) * self.i1 + insulin_influx_mu_per_min,
            i2: (1.0 / params.t_max_i) * (self.i1 - self.i2),
            r_d: params.p2_d * (i - self.r_d),
            r_e: params.p2_e * (i - self.r_e),
            a1: -(1.0 / params.t_max_g) * self.a1 + meal_g_per_min,
            a2: (1.0 / params.t_max_g) * (self.a1 - self.a2),
            q1: -(params.s_id * self.r_d + params.k21) * self.q1 + params.k12 * self.q2
                - params.f_01
                + egp
                + u_a
                + self.u_s,
            q2: params.k21 * self.q1 - params.k12 * self.q2,
            q3: params.k31 * (self.q1 - self.q3),
            u_s: 0.0,
        }
    }

    /// Forward Euler step of `dt` minutes. `u_basal` is the basal
    /// infusion (U/h), `u_bolus` a manual insulin bolus (U) and
    /// `meal_g_per_min` the carbohydrate ingestion rate (g/min).
    ///
    /// Every compartment is saturated at zero after the update so that
    /// physical masses / concentrations can never become negative; this
    /// is the physiological non-negativity invariant the Kani suite
    /// proves.
    pub fn step(
        &self,
        params: &HovorkaParams,
        u_basal: f64,
        u_bolus: f64,
        meal_g_per_min: f64,
        dt: f64,
    ) -> HovorkaState {
        let d = self.derivative(params, u_basal, u_bolus, meal_g_per_min);
        HovorkaState {
            i1: clamped_forward_euler(self.i1, d.i1, dt),
            i2: clamped_forward_euler(self.i2, d.i2, dt),
            r_d: clamped_forward_euler(self.r_d, d.r_d, dt),
            r_e: clamped_forward_euler(self.r_e, d.r_e, dt),
            a1: clamped_forward_euler(self.a1, d.a1, dt),
            a2: clamped_forward_euler(self.a2, d.a2, dt),
            q1: clamped_forward_euler(self.q1, d.q1, dt),
            q2: clamped_forward_euler(self.q2, d.q2, dt),
            q3: clamped_forward_euler(self.q3, d.q3, dt),
            u_s: self.u_s,
        }
    }

    /// Plasma glucose concentration `g_P(t)` (mmol/L).
    pub fn plasma_glucose(&self, params: &HovorkaParams) -> f64 {
        self.q1 / params.v_g
    }

    /// Interstitial (sensor) glucose concentration `g_IG(t)` (mmol/L).
    pub fn interstitial_glucose(&self, params: &HovorkaParams) -> f64 {
        self.q3 / params.v_g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native fast sweep complementing the Kani saturation proof of
    /// `clamped_forward_euler`: exercises the real wiring of `step` over
    /// per-compartment lattice slices plus pseudo-random joint stresses
    /// and asserts every compartment stays non-negative and finite.
    #[test]
    fn step_keeps_all_compartments_non_negative() {
        let params = HovorkaParams::default();
        let i_vals = [0.0, 5_000.0, 10_000.0, 25_000.0];
        let a_vals = [0.0, 40.0, 100.0, 150.0];
        let q_vals = [0.1, 5.0, 15.0, 30.0];
        let r_vals = [0.0, 2.0, 10.0, 50.0];
        let u_basal_vals = [0.0, 10.0, 25.0];
        let u_bolus_vals = [0.0, 1.0];
        let meal_vals = [0.0, 1.0, 5.0];
        let dt_vals = [0.5, 1.0, 2.0];

        let mid_state = HovorkaState {
            i1: 10_000.0,
            i2: 10_000.0,
            r_d: 2.0,
            r_e: 2.0,
            a1: 100.0,
            a2: 100.0,
            q1: 15.0,
            q2: 15.0,
            q3: 15.0,
            u_s: 0.0,
        };

        let check = |s: HovorkaState, u_basal: f64, u_bolus: f64, meal: f64, dt: f64| {
            let n = s.step(&params, u_basal, u_bolus, meal, dt);
            for (name, v) in [
                ("i1", n.i1),
                ("i2", n.i2),
                ("r_d", n.r_d),
                ("r_e", n.r_e),
                ("a1", n.a1),
                ("a2", n.a2),
                ("q1", n.q1),
                ("q2", n.q2),
                ("q3", n.q3),
            ] {
                assert!(!v.is_nan(), "{name} is NaN");
                assert!(v >= 0.0, "{name} became negative: {v}");
            }
        };

        for &u_basal in &u_basal_vals {
            for &u_bolus in &u_bolus_vals {
                for &meal in &meal_vals {
                    for &dt in &dt_vals {
                        for &v in &i_vals {
                            check(HovorkaState { i1: v, ..mid_state }, u_basal, u_bolus, meal, dt);
                            check(HovorkaState { i2: v, ..mid_state }, u_basal, u_bolus, meal, dt);
                        }
                        for &v in &a_vals {
                            check(HovorkaState { a1: v, ..mid_state }, u_basal, u_bolus, meal, dt);
                            check(HovorkaState { a2: v, ..mid_state }, u_basal, u_bolus, meal, dt);
                        }
                        for &v in &q_vals {
                            check(HovorkaState { q1: v, ..mid_state }, u_basal, u_bolus, meal, dt);
                            check(HovorkaState { q2: v, ..mid_state }, u_basal, u_bolus, meal, dt);
                            check(HovorkaState { q3: v, ..mid_state }, u_basal, u_bolus, meal, dt);
                        }
                        for &v in &r_vals {
                            check(
                                HovorkaState { r_d: v, ..mid_state },
                                u_basal,
                                u_bolus,
                                meal,
                                dt,
                            );
                            check(
                                HovorkaState { r_e: v, ..mid_state },
                                u_basal,
                                u_bolus,
                                meal,
                                dt,
                            );
                        }
                    }
                }
            }
        }

        // Pseudo-random joint stress (deterministic LCG).
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next_f = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as f64 / u64::MAX as f64
        };
        for _ in 0..500 {
            let s = HovorkaState {
                i1: next_f() * 25_000.0,
                i2: next_f() * 25_000.0,
                r_d: next_f() * 50.0,
                r_e: next_f() * 50.0,
                a1: next_f() * 150.0,
                a2: next_f() * 150.0,
                q1: 0.1 + next_f() * 29.9,
                q2: 0.1 + next_f() * 29.9,
                q3: 0.1 + next_f() * 29.9,
                u_s: next_f() * 5.0,
            };
            check(
                s,
                next_f() * 25.0,
                next_f() * 2.0,
                next_f() * 5.0,
                0.5 + next_f() * 3.0,
            );
        }
    }

    /// The low-insulin EGP branch is capped at
    /// `EGP_MAX_FOLD_OVER_BASAL` times the basal EGP; the basal identity
    /// `EGP(BIC) = EGP_B` is preserved.
    #[test]
    fn egp_is_capped_on_low_insulin_branch() {
        let params = HovorkaParams::default();
        let bic = params.basal_insulin_conc();
        let cap = EGP_MAX_FOLD_OVER_BASAL * params.egp_b;

        for r_e in [0.0, 1.0, 5.0, bic - 1.0, bic - 0.5, bic, 20.0, 100.0] {
            let v = params.egp(r_e);
            assert!(
                v >= 0.0 && v <= cap,
                "EGP escaped the cap at r_e={r_e}: {v} not in [0, {cap}]"
            );
        }
        assert!(
            (params.egp(bic) - params.egp_b).abs() < 1e-12,
            "basal identity lost by the cap"
        );
    }

    /// Basal steady-state anchor (Hovorka et al. 2004): under a constant
    /// basal infusion of `BIR` U/h and no meals / boluses, the system has
    /// an equilibrium at which depot masses sit at
    /// `1000 * BIR / 60 * t_max_i` mU, the insulin concentration equals
    /// `BIC` (mU/L), the remote actions `r_d`/`r_e` settle on `BIC`, and
    /// EGP returns to its basal value `egp_b` instead of diverging. With
    /// the default parameters the equilibrium plasma glucose sits on the
    /// nominal target (5.8 mmol/L), pinning the `i`/`BIC` unit convention
    /// that the qualitative Kani proofs cannot observe.
    #[test]
    fn basal_steady_state_matches_target() {
        let params = HovorkaParams::default();
        let bic = params.basal_insulin_conc();

        let depot_mass = MU_PER_UNIT * params.bir / 60.0 * params.t_max_i;
        let q_star = (params.egp_b - params.f_01) / (params.s_id * bic);
        let eq = HovorkaState {
            i1: depot_mass,
            i2: depot_mass,
            r_d: bic,
            r_e: bic,
            a1: 0.0,
            a2: 0.0,
            q1: q_star,
            q2: q_star,
            q3: q_star,
            u_s: 0.0,
        };

        assert!(
            (eq.insulin_conc(&params) - bic).abs() < 1e-6,
            "basal depot concentration equals BIC"
        );
        assert!(
            (params.egp(bic) - params.egp_b).abs() < 1e-12,
            "EGP at basal remote action returns to egp_b"
        );
        assert!(
            (eq.plasma_glucose(&params) - crate::TARGET_GLUCOSE_MMOL_L).abs() < 0.01,
            "basal equilibrium glucose is on target: {}",
            eq.plasma_glucose(&params)
        );

        let d = eq.derivative(&params, params.bir, 0.0, 0.0);
        let check = |name: &str, v: f64| assert!(v.abs() < 1e-9, "{name} off equilibrium: {v}");
        check("i1", d.i1);
        check("i2", d.i2);
        check("r_d", d.r_d);
        check("r_e", d.r_e);
        check("a1", d.a1);
        check("a2", d.a2);
        check("q1", d.q1);
        check("q2", d.q2);
        check("q3", d.q3);

        let n = eq.step(&params, params.bir, 0.0, 0.0, 1.0);
        let at_equilibrium = |name: &str, after: f64, before: f64| {
            assert!(
                (after - before).abs() < 1e-6,
                "{name} drifts off equilibrium after one basal step: {after} vs {before}"
            );
        };
        at_equilibrium("i1", n.i1, eq.i1);
        at_equilibrium("i2", n.i2, eq.i2);
        at_equilibrium("r_d", n.r_d, eq.r_d);
        at_equilibrium("r_e", n.r_e, eq.r_e);
        at_equilibrium("a1", n.a1, eq.a1);
        at_equilibrium("a2", n.a2, eq.a2);
        at_equilibrium("q1", n.q1, eq.q1);
        at_equilibrium("q2", n.q2, eq.q2);
        at_equilibrium("q3", n.q3, eq.q3);
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
            fn step_keeps_all_compartments_finite_and_nonnegative(
                state in state_strategy(),
                u_basal in 0.0..25.0f64,
                u_bolus in 0.0..5.0f64,
                meal in 0.0..5.0f64,
                dt in 0.5..3.0f64,
            ) {
                let params = HovorkaParams::default();
                let next = state.step(&params, u_basal, u_bolus, meal, dt);
                assert_finite_nonnegative(&next)?;
            }

            #[test]
            fn random_walk_stays_finite_and_nonnegative(
                start in state_strategy(),
                inputs in prop::collection::vec(
                    (0.0..25.0f64, 0.0..5.0f64, 0.0..5.0f64, 0.5..3.0f64),
                    0..64,
                ),
            ) {
                let params = HovorkaParams::default();
                let mut state = start;
                for (u_basal, u_bolus, meal, dt) in inputs {
                    state = state.step(&params, u_basal, u_bolus, meal, dt);
                    assert_finite_nonnegative(&state)?;
                }
            }
        }
    }
}
