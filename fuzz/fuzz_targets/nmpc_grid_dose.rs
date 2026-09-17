#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use tir_tuner_aps::controller::{
    best_grid_candidate_index, nmpc_cost, nmpc_grid_dose, NMPC_GRID_POINTS, NMPC_GRID_STEPS,
};
use tir_tuner_aps::hovorka::{HovorkaParams, HovorkaState};

#[derive(Arbitrary, Debug)]
struct Input {
    q1: f64,
    q2: f64,
    q3: f64,
    u_max: f64,
    u_operating: f64,
    target_w: f64,
    lambda: f64,
}

fn bounded(value: f64, lo: f64, hi: f64) -> Option<f64> {
    value.is_finite().then(|| value.clamp(lo, hi))
}

fuzz_target!(|input: Input| {
    let (Some(q1), Some(q2), Some(q3), Some(u_max), Some(u_operating), Some(target_w), Some(lambda)) = (
        bounded(input.q1, 0.1, 30.0),
        bounded(input.q2, 0.1, 30.0),
        bounded(input.q3, 0.1, 30.0),
        bounded(input.u_max, 0.1, 20.0),
        bounded(input.u_operating, 0.0, 20.0),
        bounded(input.target_w, 4.0, 12.0),
        bounded(input.lambda, 0.0, 10.0),
    ) else {
        return;
    };

    let params = HovorkaParams::default();
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

    let dose = nmpc_grid_dose(&params, state, u_max, u_operating, target_w, lambda);
    assert!(dose >= 0.0 && dose <= u_max, "dose out of bounds: {dose}");

    let mut rates = [0.0; NMPC_GRID_POINTS];
    let mut costs = [0.0; NMPC_GRID_POINTS];
    for k in 0..=NMPC_GRID_STEPS {
        let u = u_max * (k as f64 / NMPC_GRID_STEPS as f64);
        rates[k] = u;
        costs[k] = nmpc_cost(&params, state, u, u_operating, target_w, lambda);
        assert!(costs[k].is_finite(), "cost is not finite: {}", costs[k]);
        assert!(costs[k] >= 0.0, "cost is negative: {}", costs[k]);
    }

    let best = best_grid_candidate_index(&costs);
    assert_eq!(dose, rates[best], "dose is not the min-cost candidate");

    let min = costs.iter().cloned().fold(f64::INFINITY, f64::min);
    assert_eq!(costs[best], min, "selected candidate is not minimal");
});
