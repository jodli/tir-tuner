//! Kani formal verification suite (only built under `cargo kani`).
//!
//! Kani is used for the properties it is actually built for: universal
//! statements over bounded symbolic inputs whose operations are
//! comparisons, clamps, additions and multiplications by constants. The
//! remaining floating-point-level claims (rounding identities, dense
//! sweeps, the wired non-negativity of the numerical model) are covered
//! by the native `proptest` and exhaustive lattice tests in `hovorka.rs`,
//! `imm.rs` and `controller.rs`, plus the coverage-guided `fuzz/`
//! targets.
//!
//! # What Kani proves here
//!
//! 1. `verify_step_compartment_non_negativity` - `HovorkaState::step`
//!    routes every compartment through the `clamped_forward_euler`
//!    primitive (proved in `tir_tuner_common`) on a narrow symbolic
//!    physiological range.
//! 2. `verify_hypo_cutoff_and_dosing_bounds` - the pump dose honors the
//!    hard hypoglycemia cutoff and the `[0, u_max]` delivery bounds.
//! 3. `verify_no_floating_point_panics` - the EGP submodel never emits
//!    NaN, infinities or negative values and stays within the
//!    low-insulin-branch cap.
//! 4. `verify_mode_specific_dosing_invariants` - the Ease-off suspension
//!    rule and Boost's `+35%` intensification honor the safety envelope
//!    of the mode-aware dose calculator.
//! 5. NMPC proofs (`verify_nmpc_candidate_rates_in_bounds`,
//!    `verify_nmpc_cost_finite_nonneg`,
//!    `verify_nmpc_selection_minimal_cost`) - the NMPC dose selector of
//!    the section 5.1 cost
//!    `J(u) = sum_{j=1}^{N2} ((g_IG(t+j) - w)^2 + lambda (u - u_operating)^2)`:
//!    candidate rates lie in `[0, u_max]`, a short roll-out slice of the
//!    cost is finite and non-negative, and the position chosen by
//!    `best_grid_candidate_index` attains the minimal cost over the
//!    grid.
//! 6. `verify_imm_probability_normalization` - normalizing a
//!    non-negative mode-probability mixture keeps every entry
//!    non-negative.
//!
//! # What is deliberately not proved by Kani
//!
//! CBMC bit-blasts full-`f64` multiply/divide chains with symbolic
//! mantissas into circuits no current backend discharges in reasonable
//! time; transcendental-heavy expressions and long multi-step model
//! unrolling compound this. The historical effort to quantify the full
//! `f64` domain pushed the suite past 30 minutes. Those claims therefore
//! live in the native suite:
//!
//! * the rounding-dependent IMM sum-to-one and mixing/update identities
//!   (`proptest` plus the exhaustive divisor-16 lattice in `imm.rs`),
//! * the full-state wired non-negativity and finiteness of the model
//!   (exhaustive slices plus a `proptest` random walk in `hovorka.rs`),
//! * the full 60-minute roll-out instance the simulation drives
//!   (`proptest` in `controller.rs`; the Kani cost harness only covers a
//!   short slice), `nmpc_grid_dose`'s realized-cost composition and the
//!   composition of the grid result with the hypoglycemia guard (native
//!   `proptest` in `controller.rs`). `nmpc_grid_dose` is deliberately a
//!   pure cost minimizer: it does not see the CGM reading, so the
//!   delivered pump rate must pass through the cutoff guard
//!   (`is_hypoglycemic` / `compute_nmpc_dose_mode`), and that
//!   composition is what the native test checks.
//!
//! Out of scope (future work): the bayesian real-time adaptation of the
//! six individual dynamic parameters (section 3) and the process-noise
//! state of section 3.2F. Both would require a tractable linearization
//! or stubbing of the nonlinear model; the grid roll-out selector is the
//! largest NMPC slice kept here.

use crate::controller::{
    best_grid_candidate_index, compute_nmpc_dose, compute_nmpc_dose_mode, nmpc_cost, DosingMode,
    NMPC_GRID_POINTS, NMPC_GRID_STEPS,
};
use crate::hovorka::{egp, EGP_MAX_FOLD_OVER_BASAL, HovorkaParams, HovorkaState};
use crate::imm::normalize_imm_probabilities;
use crate::{EASE_OFF_TARGET_MMOL_L, HARD_HYPO_CUTOFF_MMOL_L, TARGET_RANGE_MAX_MMOL_L};

/// Bounds used by `verify_nmpc_cost_finite_nonneg` for the symbolic
/// glucose-state slice, matching the specification.
const MIN_GLUCOSE_MASS_MMOL_PER_KG: f64 = 0.1;
const MAX_GLUCOSE_MASS_MMOL_PER_KG: f64 = 30.0;

/// Basal plasma insulin concentration of the default parameter set
/// (1000 mU - U conversion, BIR = 1 U/h, MCR = 0.021 L/kg/min, 70 kg),
/// used as the fixed operating point of the EGP panic-freedom proof.
const REFERENCE_BIC_MU_PER_L: f64 = 1000.0 / (60.0 * 0.021 * 70.0);
/// Basal EGP of the default parameter set (mmol/kg/min).
const REFERENCE_EGP_B_MMOL_PER_KG_MIN: f64 = 0.0161;

/// Proof 2: the NMPC dose calculator never prescribes a negative rate or
/// a rate above the user-defined maximum, and it enforces the mandatory
/// zero delivery when the CGM reads below the hard hypo cutoff.
#[kani::proof]
pub fn verify_hypo_cutoff_and_dosing_bounds() {
    let ig_reading: f64 = kani::any();
    let max_delivery_rate: f64 = 10.0; // U/h limit

    kani::assume(ig_reading >= 1.0 && ig_reading <= TARGET_RANGE_MAX_MMOL_L + 14.0);

    let calculated_dose = compute_nmpc_dose(ig_reading, max_delivery_rate);

    kani::assert(calculated_dose >= 0.0, "dose is never negative");
    kani::assert(
        calculated_dose <= max_delivery_rate,
        "dose never exceeds u_max",
    );

    if ig_reading < HARD_HYPO_CUTOFF_MMOL_L {
        kani::assert(
            calculated_dose == 0.0,
            "hard hypo cutoff forces zero delivery",
        );
    }
}

/// Normalizing an arbitrary non-negative mode-probability mixture (with
/// positive total weight) keeps every entry non-negative, for symbolic
/// `f64` inputs. The rounding-dependent upper bound `<= 1.0` and the
/// sum-to-one identity are covered natively by the exhaustive divisor-16
/// lattice and the `proptest` cases in `imm.rs`.
#[kani::proof]
pub fn verify_imm_probability_normalization() {
    let mut mu: [f64; 3] = [kani::any(), kani::any(), kani::any()]; // N = 3 modes

    kani::assume(mu[0] >= 0.0 && mu[0] <= 1.0);
    kani::assume(mu[1] >= 0.0 && mu[1] <= 1.0);
    kani::assume(mu[2] >= 0.0 && mu[2] <= 1.0);
    let sum = mu[0] + mu[1] + mu[2];
    kani::assume(sum > 0.001); // Avoid division by zero

    normalize_imm_probabilities(&mut mu);

    kani::assert(mu[0] >= 0.0, "mu[0] stays non-negative");
    kani::assert(mu[1] >= 0.0, "mu[1] stays non-negative");
    kani::assert(mu[2] >= 0.0, "mu[2] stays non-negative");
}

/// Proof 4: the EGP submodel is NaN/infinity free, non-negative and
/// capped at `EGP_MAX_FOLD_OVER_BASAL` times the basal EGP over the
/// physiological remote-insulin-action domain, operating at the default
/// configuration's basal insulin concentration.
#[kani::proof]
pub fn verify_no_floating_point_panics() {
    let r_e: f64 = kani::any();
    let bic: f64 = REFERENCE_BIC_MU_PER_L;
    let egp_b: f64 = REFERENCE_EGP_B_MMOL_PER_KG_MIN;

    kani::assume(r_e >= 0.0 && r_e <= 100.0);

    let egp = egp(r_e, bic, egp_b);

    kani::assert(!egp.is_nan(), "EGP is never NaN");
    kani::assert(!egp.is_infinite(), "EGP is never infinite");
    kani::assert(egp >= 0.0, "EGP is never negative");
    kani::assert(
        egp <= EGP_MAX_FOLD_OVER_BASAL * egp_b,
        "EGP stays within the cap on the low-insulin branch",
    );
}

/// Proof 5: the mode-aware dose calculator keeps every operating mode
/// (Standard / Ease-off / Boost) inside the safety envelope:
/// `[0, u_max]` bounds, hard hypoglycemia cutoff, Ease-off suspension
/// below the elevated target and Boost never delivering less than
/// Standard.
#[kani::proof]
pub fn verify_mode_specific_dosing_invariants() {
    let ig_reading: f64 = kani::any();
    let max_delivery_rate: f64 = kani::any();

    kani::assume(ig_reading >= 0.5 && ig_reading <= 30.0);
    kani::assume(max_delivery_rate >= 0.1 && max_delivery_rate <= 20.0);

    let standard = compute_nmpc_dose_mode(ig_reading, max_delivery_rate, DosingMode::Standard);
    let ease_off = compute_nmpc_dose_mode(ig_reading, max_delivery_rate, DosingMode::EaseOff);
    let boost = compute_nmpc_dose_mode(ig_reading, max_delivery_rate, DosingMode::Boost);

    for dose in [standard, ease_off, boost] {
        kani::assert(dose >= 0.0, "dose is never negative in any mode");
        kani::assert(
            dose <= max_delivery_rate,
            "dose never exceeds u_max in any mode",
        );
    }

    if ig_reading < HARD_HYPO_CUTOFF_MMOL_L {
        kani::assert(
            standard == 0.0,
            "hard hypo cutoff forces zero Standard delivery",
        );
        kani::assert(
            ease_off == 0.0,
            "hard hypo cutoff forces zero Ease-off delivery",
        );
        kani::assert(boost == 0.0, "hard hypo cutoff forces zero Boost delivery");
    }

    if ig_reading < EASE_OFF_TARGET_MMOL_L {
        kani::assert(
            ease_off == 0.0,
            "Ease-off suspends below the elevated target",
        );
    }

    kani::assert(boost >= standard, "Boost never delivers less than Standard");
}

/// Proof 6a: every candidate rate `k / NMPC_GRID_STEPS * u_max` of the
/// one-step NMPC grid lies in `[0, u_max]`, hence so does the selected
/// dose. Pure rate arithmetic, deliberately separate from the cost
/// model so this stays cheap.
#[kani::proof]
pub fn verify_nmpc_candidate_rates_in_bounds() {
    let u_max: f64 = kani::any();
    kani::assume(u_max >= 0.1 && u_max <= 20.0);

    let mut k = 0;
    while k <= NMPC_GRID_STEPS {
        // Divide first, then multiply: `u_max * (k/N)` stays <= u_max
        // under IEEE rounding, whereas `(u_max * k) / N` can round one
        // ulp above it.
        let u = u_max * (k as f64 / NMPC_GRID_STEPS as f64);
        kani::assert(u >= 0.0 && u <= u_max, "candidate rate stays in [0, u_max]");
        k += 1;
    }
}

/// Proof 6b: a short roll-out slice of the section 5.1 NMPC cost is
/// finite and non-negative. The state is sliced to the glucose
/// compartments (`q1`, `q2`, `q3`, with the insulin/gut depots at zero)
/// plus the cost tuning knobs, and the roll-out is cut to three
/// five-minute steps (CBMC bit-blasts a long symbolic `f64` roll-out
/// into an intractable circuit); this keeps the symbolic circuit for the
/// prediction tractable while still exercising the real model, the EGP
/// and the sum-of-squares cost. Finiteness of the wider model and of the
/// full 60-minute instance the simulation drives are covered by the EGP
/// proof, the wiring proof `verify_step_compartment_non_negativity`, the
/// native `proptest` walk in `hovorka.rs` and the `proptest` cost test in
/// `controller.rs`.
#[kani::proof]
pub fn verify_nmpc_cost_finite_nonneg() {
    let q1: f64 = kani::any();
    let q2: f64 = kani::any();
    let q3: f64 = kani::any();
    let u: f64 = kani::any();
    let u_operating: f64 = kani::any();
    let target_w: f64 = kani::any();
    let lambda: f64 = kani::any();

    kani::assume(q1 >= MIN_GLUCOSE_MASS_MMOL_PER_KG && q1 <= MAX_GLUCOSE_MASS_MMOL_PER_KG);
    kani::assume(q2 >= MIN_GLUCOSE_MASS_MMOL_PER_KG && q2 <= MAX_GLUCOSE_MASS_MMOL_PER_KG);
    kani::assume(q3 >= MIN_GLUCOSE_MASS_MMOL_PER_KG && q3 <= MAX_GLUCOSE_MASS_MMOL_PER_KG);
    kani::assume(u >= 0.0 && u <= 25.0);
    kani::assume(u_operating >= 0.0 && u_operating <= 20.0);
    kani::assume(target_w >= 4.0 && target_w <= 12.0);
    kani::assume(lambda >= 0.0 && lambda <= 10.0);

    let state = HovorkaState {
        i1: 0.0,
        i2: 0.0,
        r_d: 0.0,
        r_e: 0.0,
        a1: 0.0,
        a2: 0.0,
        q1,
        q2,
        q3,
        u_s: 0.0,
    };
    let params = HovorkaParams::default();

    let cost = nmpc_cost(&params, state, u, u_operating, target_w, lambda, 15.0, 5.0);

    kani::assert(!cost.is_nan(), "NMPC cost is never NaN");
    kani::assert(!cost.is_infinite(), "NMPC cost is never infinite");
    kani::assert(cost >= 0.0, "NMPC cost is never negative");
}

/// Proof 6c: `best_grid_candidate_index` returns the position of the
/// minimal cost among the candidates. The costs are abstracted with
/// `kani::any()` so the proof is pure selection logic over a bounded
/// `f64` array and stays fast; it does not inline the physiological
/// model.
#[kani::proof]
pub fn verify_nmpc_selection_minimal_cost() {
    let mut costs = [0.0; NMPC_GRID_POINTS];
    let mut i = 0;
    while i < NMPC_GRID_POINTS {
        costs[i] = kani::any();
        kani::assume(costs[i] >= -10.0 && costs[i] <= 10.0); // finite, no NaN/infinity
        i += 1;
    }

    let best = best_grid_candidate_index(&costs);

    let mut j = 0;
    while j < NMPC_GRID_POINTS {
        kani::assert(
            costs[best] <= costs[j],
            "selected candidate attains the minimal cost",
        );
        j += 1;
    }
}

/// `HovorkaState::step` routes every compartment through the
/// `clamped_forward_euler` saturation primitive, so each compartment
/// stays non-negative for a bounded symbolic physiological state and
/// bounded inputs (basal/bolus insulin, meal ingestion rate and the
/// external process-noise influx `u_s`). This complements the primitive
/// proof above with the actual wiring of the model; the unbounded state
/// space is covered by the native `proptest` walk in `hovorka.rs`.
#[kani::proof]
pub fn verify_step_compartment_non_negativity() {
    let i1: f64 = kani::any();
    let i2: f64 = kani::any();
    let r_d: f64 = kani::any();
    let r_e: f64 = kani::any();
    let a1: f64 = kani::any();
    let a2: f64 = kani::any();
    let q1: f64 = kani::any();
    let q2: f64 = kani::any();
    let q3: f64 = kani::any();
    let u_s: f64 = kani::any();
    let u_basal: f64 = kani::any();
    let u_bolus: f64 = kani::any();
    let meal: f64 = kani::any();

    kani::assume(i1 >= 0.0 && i1 <= 25_000.0);
    kani::assume(i2 >= 0.0 && i2 <= 25_000.0);
    kani::assume(r_d >= 0.0 && r_d <= 50.0);
    kani::assume(r_e >= 0.0 && r_e <= 50.0);
    kani::assume(a1 >= 0.0 && a1 <= 150.0);
    kani::assume(a2 >= 0.0 && a2 <= 150.0);
    kani::assume(q1 >= 0.0 && q1 <= 30.0);
    kani::assume(q2 >= 0.0 && q2 <= 30.0);
    kani::assume(q3 >= 0.0 && q3 <= 30.0);
    kani::assume(u_s >= 0.0 && u_s <= 5.0);
    kani::assume(u_basal >= 0.0 && u_basal <= 25.0);
    kani::assume(u_bolus >= 0.0 && u_bolus <= 5.0);
    kani::assume(meal >= 0.0 && meal <= 5.0);

    let state = HovorkaState {
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
    };
    let params = HovorkaParams::default();

    let next = state.step(&params, u_basal, u_bolus, meal, 1.0);

    kani::assert(next.i1 >= 0.0, "i1 stays non-negative");
    kani::assert(next.i2 >= 0.0, "i2 stays non-negative");
    kani::assert(next.r_d >= 0.0, "r_d stays non-negative");
    kani::assert(next.r_e >= 0.0, "r_e stays non-negative");
    kani::assert(next.a1 >= 0.0, "a1 stays non-negative");
    kani::assert(next.a2 >= 0.0, "a2 stays non-negative");
    kani::assert(next.q1 >= 0.0, "q1 stays non-negative");
    kani::assert(next.q2 >= 0.0, "q2 stays non-negative");
    kani::assert(next.q3 >= 0.0, "q3 stays non-negative");
}
