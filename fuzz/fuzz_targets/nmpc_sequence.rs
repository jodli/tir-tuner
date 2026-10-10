#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use tir_tuner_aps::controller::{Nmpc, trajectory};
use tir_tuner_common::settings::AUTO_TARGET_MMOL_L;
use tir_tuner_aps::hovorka::{HovorkaParams, HovorkaState};

#[derive(Arbitrary, Debug)]
struct Input {
    q1: f64,
    q2: f64,
    q3: f64,
    r_d: f64,
    r_e: f64,
    target: f64,
    u_max: f64,
    u_prev: f64,
    k_agr: f64,
}

fn bounded(value: f64, lo: f64, hi: f64) -> Option<f64> {
    value.is_finite().then(|| value.clamp(lo, hi))
}

fuzz_target!(|input: Input| {
    let (
        Some(q1),
        Some(q2),
        Some(q3),
        Some(r_d),
        Some(r_e),
        Some(target),
        Some(u_max),
        Some(u_prev),
        Some(k_agr),
    ) = (
        bounded(input.q1, 0.1, 30.0),
        bounded(input.q2, 0.1, 30.0),
        bounded(input.q3, 0.1, 30.0),
        bounded(input.r_d, 0.0, 50.0),
        bounded(input.r_e, 0.0, 50.0),
        bounded(input.target, AUTO_TARGET_MMOL_L.0, AUTO_TARGET_MMOL_L.1),
        bounded(input.u_max, 0.1, 20.0),
        bounded(input.u_prev, 0.0, 20.0),
        bounded(input.k_agr, 0.5, 50.0),
    )
    else {
        return;
    };

    let params = HovorkaParams::from_weight_and_tdd(70.0, 50.0);
    let state = HovorkaState {
        r_d,
        r_e,
        q1,
        q2,
        q3,
        ..HovorkaState::basal(&params, 6.0)
    };
    let nmpc = Nmpc {
        k_agr,
        u_max,
        n: 6,
        ..Nmpc::default()
    };
    let w = trajectory(state.plasma_glucose(&params), target, nmpc.n, nmpc.step_min);
    let (_, seed_cost) = nmpc.seed(&params, state, &w, u_prev);
    assert!(seed_cost.is_finite() && seed_cost >= 0.0, "seed cost {seed_cost}");
    let u = nmpc.sequence(&params, state, target, u_prev);
    assert_eq!(u.len(), nmpc.n);
    assert!(u.iter().all(|r| (0.0..=u_max).contains(r)), "{u:?}");
    assert!(nmpc.cost(&params, state, &u, &w, u_prev) <= seed_cost);
});
