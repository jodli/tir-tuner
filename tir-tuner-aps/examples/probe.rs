//! Probe the aps dose selector on a hyperglycemic admit.
//! `cargo run -p tir-tuner-aps --example probe`

use tir_tuner_aps::controller::{moving_target_trajectory, nmpc_sequence_cost, nmpc_sequence_dose};
use tir_tuner_aps::hovorka::{HovorkaParams, HovorkaState};
use tir_tuner_aps::TARGET_GLUCOSE_MMOL_L;

fn state_10() -> HovorkaState {
    HovorkaState {
        i1: 0.0,
        i2: 0.0,
        r_d: 0.0,
        r_e: 0.0,
        a1: 0.0,
        a2: 0.0,
        q1: 10.0 * 0.9,
        q2: 10.0 * 0.9,
        q3: 10.0 * 1.0,
        u_s: 0.0,
    }
}

fn main() {
    let params = HovorkaParams::default();
    let state = state_10();
    println!("v_g={}", params.v_g);

    for k_agr in [0.5, 1.0, 5.0, 40.0] {
        let dose = nmpc_sequence_dose(
            &params,
            state,
            10.0,
            TARGET_GLUCOSE_MMOL_L,
            k_agr,
            20.0,
            1.0,
            240.0,
            15.0,
        );
        println!("k_agr {k_agr}: dose {dose:.3}");
    }

    // What does the cost look like as a function of constant rate?
    let w = moving_target_trajectory(10.0, TARGET_GLUCOSE_MMOL_L, 16, 15.0);
    println!("trajectory: {w:?}");
    for u in [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 7.5, 10.0] {
        let mut seq = vec![0.0; 16];
        seq.fill(u);
        let c = nmpc_sequence_cost(&params, state, &seq, &w, 1.0, 40.0, 15.0);
        println!("u {u:>4}: cost {c:.2}");
    }
}