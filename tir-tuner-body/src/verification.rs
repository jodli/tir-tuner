//! Kani formal verification suite for the body model (only built under
//! `cargo kani`).
//!
//! Follows the aps-crate policy: universal statements over bounded
//! symbolic inputs whose operations are comparisons, clamps, additions
//! and multiplications by constants. The wired non-negativity of the
//! clamped Euler integration is proved symbolically over a narrow
//! physiological band; the full-state behavior is covered by the native
//! `proptest` walk in `solver.rs`.
//!
//! # Source
//!
//! The body integrates the Cambridge simulator subject of `[W10]`: the
//! EGP suppression, the saturable `F01c` and the renal-excretion
//! threshold are the blueprint section 3.2D forms of the Hovorka model
//! (`[W04]`).

use crate::derivative::{egp, f01c, gut_appearance, renal_excretion, BodyInputs};
use crate::solver::step;
use crate::state::BodyState;
use crate::subject::VirtualSubject;

/// Symbolic physiological band used by the step proof, mirroring the
/// plausible ranges of every compartment after an admit/adjust step.
fn symbolic_subject() -> VirtualSubject {
    VirtualSubject::population_mean()
}

/// Proof 1: the EGP submodel stays capped and non-negative over the
/// physiological insulin-action range. The exact basal anchor (EGP
/// equals `egp0` at the basal action) is a transcendental identity that
/// CBMC cannot discharge from its `exp` model, so it is covered by the
/// native test `derivative::tests::egp_suppresses_with_insulin_action`.
#[kani::proof]
pub fn verify_egp_suppression_bounds() {
    let subject = symbolic_subject();
    let x3: f64 = kani::any();
    kani::assume(x3 >= 0.0 && x3 <= 100.0);

    let egp_value = egp(&subject, x3);
    kani::assert(!egp_value.is_nan(), "EGP is never NaN");
    kani::assert(!egp_value.is_infinite(), "EGP is never infinite");
    kani::assert(egp_value >= 0.0, "EGP is never negative");
    kani::assert(
        egp_value <= 3.0 * subject.egp0_mmol_per_kg_min,
        "EGP stays within the suppression cap",
    );
    // The basal action is inside the covered domain, so the cap and
    // non-negativity statements already hold for it; anchor equality is
    // a native-test claim per the crate division of labor.
}

/// Proof 2: F01c is non-negative and never exceeds the saturation
/// `F01/0.85`, and renal excretion is identically zero at and below the
/// threshold.
#[kani::proof]
pub fn verify_f01_and_renal_bounds() {
    let subject = symbolic_subject();
    let g: f64 = kani::any();
    kani::assume(g >= 0.0 && g <= 30.0);

    let f01 = f01c(&subject, g);
    kani::assert(f01 >= 0.0, "F01c is never negative");
    kani::assert(
        f01 <= subject.f01_mmol_per_kg_min / 0.85 + 1e-12,
        "F01c never exceeds the saturation",
    );

    if g <= subject.r_thr_mmol_per_l {
        kani::assert(
            renal_excretion(&subject, g) == 0.0,
            "renal excretion zero at and below the threshold",
        );
    }
}

/// Proof 3: gut appearance is capped at `ug_ceil` and non-negative.
#[kani::proof]
pub fn verify_gut_appearance_capped() {
    let subject = symbolic_subject();
    let g2: f64 = kani::any();
    kani::assume(g2 >= 0.0 && g2 <= 500.0);

    let ug = gut_appearance(&subject, g2);
    kani::assert(ug >= 0.0, "gut appearance is never negative");
    kani::assert(
        ug <= subject.ug_ceil_mmol_per_kg_min + 1e-12,
        "gut appearance never exceeds the cap",
    );
}

/// Proof 4: one clamped Euler step keeps every compartment
/// non-negative and finite on a bounded symbolic physiological state and
/// bounded inputs. This proves the wiring of the full 11-compartment
/// step against the `clamped_forward_euler` primitive proved in the
/// common crate.
#[kani::proof]
pub fn verify_step_compartment_non_negativity() {
    let subject = symbolic_subject();
    let s1: f64 = kani::any();
    let s2: f64 = kani::any();
    let i: f64 = kani::any();
    let x1: f64 = kani::any();
    let x2: f64 = kani::any();
    let x3: f64 = kani::any();
    let q1: f64 = kani::any();
    let q2: f64 = kani::any();
    let g1: f64 = kani::any();
    let g2: f64 = kani::any();
    let c: f64 = kani::any();
    let u: f64 = kani::any();
    let meal: f64 = kani::any();

    kani::assume(s1 >= 0.0 && s1 <= 100.0);
    kani::assume(s2 >= 0.0 && s2 <= 100.0);
    kani::assume(i >= 0.0 && i <= 200.0);
    kani::assume(x1 >= 0.0 && x1 <= 200.0);
    kani::assume(x2 >= 0.0 && x2 <= 200.0);
    kani::assume(x3 >= 0.0 && x3 <= 200.0);
    kani::assume(q1 >= 0.1 && q1 <= 20.0);
    kani::assume(q2 >= 0.0 && q2 <= 20.0);
    kani::assume(g1 >= 0.0 && g1 <= 150.0);
    kani::assume(g2 >= 0.0 && g2 <= 150.0);
    kani::assume(c >= 0.0 && c <= 20.0);
    kani::assume(u >= 0.0 && u <= 500.0);
    kani::assume(meal >= 0.0 && meal <= 5.0);

    let state = BodyState {
        s1,
        s2,
        i,
        x1,
        x2,
        x3,
        q1,
        q2,
        g1,
        g2,
        c,
    };
    let inputs = BodyInputs {
        u_basal_mu_per_min: u,
        meal_g_per_min: meal,
    };

    let next = step(&subject, &state, &inputs, 1.0);

    kani::assert(next.s1 >= 0.0, "s1 stays non-negative");
    kani::assert(next.s2 >= 0.0, "s2 stays non-negative");
    kani::assert(next.i >= 0.0, "i stays non-negative");
    kani::assert(next.x1 >= 0.0, "x1 stays non-negative");
    kani::assert(next.x2 >= 0.0, "x2 stays non-negative");
    kani::assert(next.x3 >= 0.0, "x3 stays non-negative");
    kani::assert(next.q1 >= 0.0, "q1 stays non-negative");
    kani::assert(next.q2 >= 0.0, "q2 stays non-negative");
    kani::assert(next.g1 >= 0.0, "g1 stays non-negative");
    kani::assert(next.g2 >= 0.0, "g2 stays non-negative");
    kani::assert(next.c >= 0.0, "c stays non-negative");
    for v in [
        next.s1, next.s2, next.i, next.x1, next.x2, next.x3, next.q1, next.q2, next.g1, next.g2,
        next.c,
    ] {
        kani::assert(!v.is_nan() && !v.is_infinite(), "clamped step stays finite");
    }
}