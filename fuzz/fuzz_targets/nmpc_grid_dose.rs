#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use tir_tuner_aps::controller::{
    best_grid_candidate_index, nmpc_constant_cost, nmpc_grid_dose, nmpc_sequence,
    nmpc_sequence_cost, NMPC_GRID_POINTS, NMPC_GRID_STEPS,
};
use tir_tuner_aps::hovorka::{HovorkaParams, HovorkaState};

#[derive(Arbitrary, Debug)]
struct Input {
    q1: f64,
    q2: f64,
    q3: f64,
    r_d: f64,
    r_e: f64,
    y_meas: f64,
    target: f64,
    u_max: f64,
    u_prev: f64,
    k_agr: f64,
    step_min: f64,
}

fn bounded(value: f64, lo: f64, hi: f64) -> Option<f64> {
    value.is_finite().then(|| value.clamp(lo, hi))
}

fn trajectory(n: usize, y_meas: f64, target: f64, _step_min: f64) -> Vec<f64> {
    // A straight reference line from the measured glucose to the target,
    // enough for the sequence-level invariants; the aps crate's own
    // trajectory generator is exercised by the Kani and unit tests.
    (0..n)
        .map(|j| {
            let frac = (j as f64) / (n as f64).max(1.0);
            y_meas + (target - y_meas) * frac
        })
        .collect()
}

fuzz_target!(|input: Input| {
    let (Some(q1), Some(q2), Some(q3), Some(r_d), Some(r_e), Some(y_meas), Some(target), Some(u_max), Some(u_prev), Some(k_agr), Some(step_min)) = (
        bounded(input.q1, 0.1, 30.0),
        bounded(input.q2, 0.1, 30.0),
        bounded(input.q3, 0.1, 30.0),
        bounded(input.r_d, 0.0, 50.0),
        bounded(input.r_e, 0.0, 50.0),
        bounded(input.y_meas, 2.0, 20.0),
        bounded(input.target, 4.0, 12.0),
        bounded(input.u_max, 0.1, 20.0),
        bounded(input.u_prev, 0.0, 20.0),
        bounded(input.k_agr, 0.5, 50.0),
        bounded(input.step_min, 5.0, 30.0),
    ) else {
        return;
    };

    let params = HovorkaParams::default();
    let state = HovorkaState {
        i1: 0.0,
        i2: 0.0,
        r_d,
        r_e,
        a1: 0.0,
        a2: 0.0,
        q1,
        q2,
        q3,
        u_s: 0.0,
    };

    let n = 6;
    let w = trajectory(n, y_meas, target, step_min);

    let dose = nmpc_grid_dose(&params, state, &w, u_prev, u_max, k_agr, step_min);
    assert!(dose >= 0.0 && dose <= u_max, "grid dose out of bounds: {dose}");

    let mut rates = [0.0; NMPC_GRID_POINTS];
    let mut costs = [0.0; NMPC_GRID_POINTS];
    for k in 0..=NMPC_GRID_STEPS {
        let u = u_max * (k as f64 / NMPC_GRID_STEPS as f64);
        rates[k] = u;
        costs[k] = nmpc_constant_cost(&params, state, u, &w, u_prev, k_agr, step_min);
        assert!(costs[k].is_finite(), "grid cost is not finite: {}", costs[k]);
        assert!(costs[k] >= 0.0, "grid cost is negative: {}", costs[k]);
    }

    let best = best_grid_candidate_index(&costs);
    assert_eq!(dose, rates[best], "dose is not the min-cost grid candidate");
    let min = costs.iter().cloned().fold(f64::INFINITY, f64::min);
    assert_eq!(costs[best], min, "selected grid candidate is not minimal");

    let seq = nmpc_sequence(&params, state, y_meas, target, k_agr, u_max, u_prev, n as f64 * step_min, step_min);
    assert_eq!(seq.len(), n, "sequence length mismatch");
    for &u in &seq {
        assert!(u.is_finite(), "sequence rate not finite: {u}");
        assert!((0.0..=u_max).contains(&u), "sequence rate out of bounds: {u}");
    }
    let seq_cost = nmpc_sequence_cost(&params, state, &seq, &w, u_prev, k_agr, step_min);
    assert!(seq_cost.is_finite(), "sequence cost not finite: {seq_cost}");
    assert!(seq_cost >= 0.0, "sequence cost negative: {seq_cost}");
});