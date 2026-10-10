//! Kani proofs (`cargo kani -p tir-tuner-aps`).
//!
//! Kani proves what it handles well: bounded symbolic inputs through
//! comparisons, clamps and short arithmetic. Long `f64` chains, the full
//! horizon and the IMM rounding identities are covered by the proptests in
//! `hovorka.rs`, `controller.rs`, `imm.rs` and the `fuzz/` targets.

use crate::controller::{
    EASE_OFF_SUSPEND_MMOL_L, GRID_POINTS, GRID_STEPS, HYPO_CUTOFF_MMOL_L, Mode, Nmpc,
    ROLLOUT_STEP_MIN, TRAJECTORY_STEEP_ABOVE_MMOL_L, argmin, trajectory,
};
use crate::hovorka::{HovorkaParams, HovorkaState, egp};
use crate::imm::imm_mode_probability_update;
use tir_tuner_common::settings::AUTO_TARGET_MMOL_L;

/// Above any CGM reading in mmol/L.
const READING_MAX_MMOL_L: f64 = 30.0;

fn params() -> HovorkaParams {
    HovorkaParams::hovorka_2004(70.0, 1.0)
}

/// Every Auto mode variant delivers within `[0, u_max]`, nothing below the
/// hypo cutoff, nothing in Ease-off below its suspension, and Boost never
/// less than Standard.
#[kani::proof]
pub fn verify_mode_rate_invariants() {
    let nmpc: f64 = kani::any();
    let reading: f64 = kani::any();
    let target: f64 = kani::any();
    let u_max: f64 = kani::any();
    kani::assume(nmpc >= 0.0 && nmpc <= 20.0);
    kani::assume(reading >= 0.0 && reading <= READING_MAX_MMOL_L);
    kani::assume(target >= AUTO_TARGET_MMOL_L.0 && target <= AUTO_TARGET_MMOL_L.1);
    kani::assume(u_max >= 0.1 && u_max <= 20.0);

    let standard = Mode::Standard.rate(nmpc, reading, target, u_max);
    let ease_off = Mode::EaseOff.rate(nmpc, reading, target, u_max);
    let boost = Mode::Boost.rate(nmpc, reading, target, u_max);
    for r in [standard, ease_off, boost] {
        kani::assert(r >= 0.0 && r <= u_max, "rate in [0, u_max]");
        if reading < HYPO_CUTOFF_MMOL_L {
            kani::assert(r == 0.0, "nothing below the hypo cutoff");
        }
    }
    if reading < EASE_OFF_SUSPEND_MMOL_L {
        kani::assert(ease_off == 0.0, "Ease-off suspends below 7.0");
    }
    kani::assert(boost >= standard, "Boost never below Standard");
}

/// EGP is finite and non-negative over the remote action range.
#[kani::proof]
pub fn verify_egp_finite_nonneg() {
    let r_e: f64 = kani::any();
    kani::assume(r_e >= 0.0 && r_e <= 100.0);
    let p = params();
    let v = egp(r_e, p.basal_insulin_conc(), p.egp_b, p.i_half);
    kani::assert(v.is_finite() && v >= 0.0, "EGP finite and non-negative");
}

/// Every seed grid candidate lies in `[0, u_max]`.
#[kani::proof]
pub fn verify_grid_rates_in_bounds() {
    let u_max: f64 = kani::any();
    kani::assume(u_max >= 0.1 && u_max <= 20.0);
    let nmpc = Nmpc {
        u_max,
        ..Nmpc::default()
    };
    for k in 0..=GRID_STEPS {
        let u = nmpc.grid_rate(k);
        kani::assert(u >= 0.0 && u <= u_max, "candidate in [0, u_max]");
    }
}

/// The selected grid position has the minimal cost.
#[kani::proof]
pub fn verify_argmin_is_minimal() {
    let costs: [f64; GRID_POINTS] = kani::any();
    for c in costs {
        kani::assume(c >= -10.0 && c <= 10.0);
    }
    let best = argmin(&costs);
    for c in costs {
        kani::assert(costs[best] <= c, "selected cost is minimal");
    }
}

/// A two-sample eq 9 cost is finite and non-negative. One-minute samples
/// keep the symbolic rollout to two model steps.
#[kani::proof]
pub fn verify_cost_finite_nonneg() {
    let q1: f64 = kani::any();
    let q2: f64 = kani::any();
    let q3: f64 = kani::any();
    let u: [f64; 2] = kani::any();
    let w: [f64; 3] = kani::any();
    let u_prev: f64 = kani::any();
    for q in [q1, q2, q3] {
        kani::assume(q >= 0.1 && q <= 30.0);
    }
    for r in u {
        kani::assume(r >= 0.0 && r <= 20.0);
    }
    for g in w {
        kani::assume(g >= 2.0 && g <= 15.0);
    }
    kani::assume(u_prev >= 0.0 && u_prev <= 20.0);

    let p = params();
    let state = HovorkaState {
        q1,
        q2,
        q3,
        ..HovorkaState::basal(&p, 6.0)
    };
    let nmpc = Nmpc {
        step_min: ROLLOUT_STEP_MIN,
        n: 2,
        ..Nmpc::default()
    };
    let cost = nmpc.cost(&p, state, &u, &w, u_prev);
    kani::assert(
        cost.is_finite() && cost >= 0.0,
        "cost finite and non-negative",
    );
}

/// Far above target the trajectory falls monotonically and never below
/// the target (H04 section 3.3).
#[kani::proof]
pub fn verify_trajectory_decline_bounded() {
    let target = 6.0;
    let y: f64 = kani::any();
    kani::assume(y >= target + 2.0 * TRAJECTORY_STEEP_ABOVE_MMOL_L && y <= 16.0);
    let w = trajectory(y, target, 3, 15.0);
    for i in 1..w.len() {
        kani::assert(w[i].is_finite(), "finite");
        kani::assert(
            w[i] >= target && w[i] <= w[i - 1],
            "monotone, not below target",
        );
    }
}

/// Eq 80 with non-negative weights and likelihoods gives non-negative
/// mode probabilities.
#[kani::proof]
pub fn verify_imm_mode_update_non_negative() {
    let c: [f64; 3] = kani::any();
    let l: [f64; 3] = kani::any();
    for (c, l) in c.iter().zip(&l) {
        kani::assume(*c >= 0.0 && *c <= 1.0 && *l >= 0.0 && *l <= 1.0);
    }
    if let Some(mu) = imm_mode_probability_update(&l, &c) {
        for m in mu {
            kani::assert(m >= 0.0, "stays non-negative");
        }
    }
}

/// `step` clamps every compartment at zero.
#[kani::proof]
pub fn verify_step_non_negative() {
    let s = HovorkaState {
        i1: kani::any(),
        i2: kani::any(),
        r_d: kani::any(),
        r_e: kani::any(),
        a1: kani::any(),
        a2: kani::any(),
        q1: kani::any(),
        q2: kani::any(),
        q3: kani::any(),
        u_s: kani::any(),
    };
    for (v, hi) in [
        (s.i1, 25_000.0),
        (s.i2, 25_000.0),
        (s.r_d, 50.0),
        (s.r_e, 50.0),
        (s.a1, 150.0),
        (s.a2, 150.0),
        (s.q1, 30.0),
        (s.q2, 30.0),
        (s.q3, 30.0),
        (s.u_s, 5.0),
    ] {
        kani::assume(v >= 0.0 && v <= hi);
    }
    let u_basal: f64 = kani::any();
    let meal: f64 = kani::any();
    kani::assume(u_basal >= 0.0 && u_basal <= 25.0);
    kani::assume(meal >= 0.0 && meal <= 5.0);

    let n = s.step(&params(), u_basal, meal, 1.0);
    for v in [n.i1, n.i2, n.r_d, n.r_e, n.a1, n.a2, n.q1, n.q2, n.q3] {
        kani::assert(v >= 0.0, "compartment non-negative");
    }
}
