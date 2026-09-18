//! Sweep the aggressiveness constant of the section 5.1 NMPC over the
//! hermetic four-meal day and print TIR, percent below range, mean and
//! the closed-loop pull-down at a high admit. Run:
//! `cargo run -p tir-tuner-cli --example kagr_sweep`

use tir_tuner_aps::controller::{
    moving_target_trajectory, nmpc_constant_cost, nmpc_grid_dose, nmpc_sequence_cost,
    nmpc_sequence_dose, nmpc_sequence,
};
use tir_tuner_aps::TARGET_GLUCOSE_MMOL_L;
use tir_tuner_cli::engine::{
    belief_at_glucose, controller_model, simulate, Meal, SimConfig,
};
use tir_tuner_common::metrics::TIME_IN_RANGE_MIN_MMOL_L;
use tir_tuner_common::units::mg_per_dl_to_mmol_per_l;
use tir_tuner_common::units::MU_PER_UNIT;

fn metrics(trace: &tir_tuner_cli::engine::SimTrace) -> (f64, f64, f64) {
    let n = trace.reading_mmol_per_l.len() as f64;
    let tir = 100.0
        * trace
            .reading_mmol_per_l
            .iter()
            .filter(|&&v| v >= TIME_IN_RANGE_MIN_MMOL_L && v <= 10.0)
            .count() as f64
        / n;
    let low = 100.0
        * trace
            .reading_mmol_per_l
            .iter()
            .filter(|&&v| v < TIME_IN_RANGE_MIN_MMOL_L)
            .count() as f64
        / n;
    let mean = trace.reading_mmol_per_l.iter().sum::<f64>() / n;
    (tir, low, mean)
}

fn main() {
    let mut rest = SimConfig::default();
    rest.duration_hours = 48.0;
    rest.admit_glucose_mg_per_dl = 100.0;
    rest.use_controller = false;
    rest.announce_meals = false;
    rest.pump_error_cv = 0.0;
    let t = simulate(&rest);
    let last = t.interstitial_mmol_per_l.last().copied().unwrap();
    let end5 = t.interstitial_mmol_per_l[t.interstitial_mmol_per_l.len() - 5..].to_vec();
    println!(
        "body rest (open loop 48h): {last:.2}  trailing {:?}",
        end5.iter().map(|v| format!("{v:.2}")).collect::<Vec<_>>()
    );
    let subject = tir_tuner_cli::engine::SimConfig::default().subject;
    let params = controller_model(&subject);

    println!(
        "belief params: s_id={:.6} bic={:.2} egp={:.4} f01={:.4} mcr_i={:.4} vg={:.4} rest-math={:.2}",
        params.s_id,
        params.basal_insulin_conc(),
        params.egp_b,
        params.f_01,
        params.mcr_i,
        params.v_g,
        (params.egp_b - params.f_01) / (params.s_id * params.basal_insulin_conc()),
    );
    let defaults = tir_tuner_aps::hovorka::HovorkaParams::default();
    println!(
        "default-aps params: s_id={:.6} bic={:.2} egp={:.4} f01={:.4} mcr_i={:.4} vg={:.4} rest-math={:.2}",
        defaults.s_id,
        defaults.basal_insulin_conc(),
        defaults.egp_b,
        defaults.f_01,
        defaults.mcr_i,
        defaults.v_g,
        (defaults.egp_b - defaults.f_01) / (defaults.s_id * defaults.basal_insulin_conc()),
    );

    for (label, initial) in [("admit-10", mg_per_dl_to_mmol_per_l(180.0))] {
        let mut s = belief_at_glucose(&params, &subject, initial);
        let mut profile = Vec::new();
        for _ in 0..16 {
            s = s.step(&params, subject.bir_u_per_h, 0.0, 0.0, 15.0);
            profile.push(s.interstitial_glucose(&params));
        }
        println!(
            "{label} basal-step profile: {:?}",
            profile.iter().map(|v| format!("{v:.2}")).collect::<Vec<_>>().join(" ")
        );
    }

    let rest_belief = belief_at_glucose(&params, &subject, TARGET_GLUCOSE_MMOL_L);
    for (label, u) in [("basal", subject.bir_u_per_h), ("+0.5", subject.bir_u_per_h + 0.5), ("+1.0", subject.bir_u_per_h + 1.0)] {
        let mut s = rest_belief;
        let mut profile = Vec::new();
        for _ in 0..16 {
            s = s.step(&params, u, 0.0, 0.0, 15.0);
            profile.push(s.interstitial_glucose(&params));
        }
        println!("belief rest 5.8, u={label}: {:?}", profile.iter().map(|v| format!("{v:.2}")).collect::<Vec<_>>().join(" "));
    }

    // Scan the belief's s_id, re-resting it on the body's resting glucose
    // (9.58), to see whether any sensitivity matches the body's +0.5
    // step response or the crash is structural.
    for (s_scale, label) in [(0.30, "0.30x"), (0.50, "0.50x"), (0.70, "0.70x"), (1.00, "1.00x")] {
        let mut p = params;
        p.s_id = params.s_id * s_scale;
        let mut s = belief_at_glucose(&p, &subject, 9.58);
        for _ in 0..(8 * 4) as usize {
            s = s.step(&p, subject.bir_u_per_h, 0.0, 0.0, 15.0);
        }
        let rest = s.interstitial_glucose(&p);
        let mut s2 = s;
        let mut profile = Vec::new();
        for _ in 0..32 {
            s2 = s2.step(&p, subject.bir_u_per_h + 0.5, 0.0, 0.0, 15.0);
            profile.push(s2.interstitial_glucose(&p));
        }
        println!(
            "belief s_id {label} rest {rest:.2} then +0.5 8h: {}",
            profile.iter().enumerate().filter(|(i, _)| i % 4 == 3).map(|(_, v)| format!("{v:.2}")).collect::<Vec<_>>().join(" ")
        );
    }

    for (label, admit_mg) in [("100", 100.0), ("140", 140.0)] {
        let mut cfg = SimConfig::default();
        cfg.duration_hours = 48.0;
        cfg.admit_glucose_mg_per_dl = admit_mg;
        cfg.use_controller = false;
        cfg.announce_meals = false;
        cfg.pump_error_cv = 0.0;
        cfg.max_delivery_u_per_h = 20.0;
        let t = simulate(&cfg);
        let last = t.interstitial_mmol_per_l.last().copied().unwrap();
        println!("body rest admit {label}: {last:.2}");
    }
    // Body open-loop step response from rest at basal+0.5 and basal+1.0.
    for (label, rate) in [("basal+0.5", 1.5), ("basal+1.0", 2.0), ("basal-0.3", 0.7)] {
        let mut cfg = SimConfig::default();
        cfg.duration_hours = 8.0;
        cfg.admit_glucose_mg_per_dl = 140.0;
        cfg.use_controller = false;
        cfg.announce_meals = false;
        cfg.pump_error_cv = 0.0;
        cfg.max_delivery_u_per_h = 20.0;
        let mut t = simulate(&cfg);
        // Rewrite the delivery trace to a constant non-basal rate by
        // injecting via a custom run: instead trust open-loop basal for
        // the first 2h then compare; approximate by using the controller
        // disabled means basal only, so here just patch: we cannot
        // inject arbitrary rate via SimConfig, so compute manually below.
        let _ = &mut t;
        let _ = rate;
        let _ = label;
    }
    // Simulate the body at a fixed non-basal rate by re-running the body
    // integrator directly with BodyInputs.
    let mut s = tir_tuner_body::solver::admit_state(&subject, 140.0);
    let inputs_base = tir_tuner_body::derivative::BodyInputs {
        u_basal_mu_per_min: subject.bir_u_per_h / 60.0 * MU_PER_UNIT,
        meal_g_per_min: 0.0,
    };
    let dt = tir_tuner_body::solver::DT_MIN;
    let tm = 480.0;
    for _ in 0..(tm / dt) as usize {
        s = tir_tuner_body::solver::step(&subject, &s, &inputs_base, dt);
    }
    println!("body settle from 140 baseline: {:.2}", s.c);
    for (label, rate) in [("basal+0.5", subject.bir_u_per_h + 0.5), ("basal+1.0", subject.bir_u_per_h + 1.0)] {
        let mut s2 = s;
        let inputs = tir_tuner_body::derivative::BodyInputs {
            u_basal_mu_per_min: rate / 60.0 * MU_PER_UNIT,
            meal_g_per_min: 0.0,
        };
        let mut end = Vec::new();
        for k in 0..(tm / dt) as usize {
            s2 = tir_tuner_body::solver::step(&subject, &s2, &inputs, dt);
            if k % 60 == 59 {
                end.push(format!("{:.2}", s2.c));
            }
        }
        println!("body {label} 8h hourly: {}", end.join(" "));
    }
    let mut cfg = SimConfig::default();
    cfg.duration_hours = 24.0;
    cfg.admit_glucose_mg_per_dl = 140.0;
    cfg.use_controller = false;
    cfg.announce_meals = false;
    cfg.pump_error_cv = 0.0;
    cfg.max_delivery_u_per_h = 20.0;
    cfg.meals = vec![Meal { start_min: 60.0, carbs_g: 60.0, duration_min: 15.0 }];
    let t = simulate(&cfg);
    for (i, &g) in t.interstitial_mmol_per_l.iter().enumerate() {
        if i % 16 == 0 {
            println!("body open loop +60g meal t={}min: {g:.2}", i * 15);
        }
    }

    let mut s = belief_at_glucose(&params, &subject, mg_per_dl_to_mmol_per_l(180.0));
    let mut profile = Vec::new();
    for _ in 0..16 {
        s = s.step(&params, subject.bir_u_per_h, 0.0, 0.0, 15.0);
        let own = s.interstitial_glucose(&params);
        let blended = 0.5 * s.interstitial_glucose(&params) + 0.5 * own;
        s.q1 = blended * params.v_g;
        s.q2 = blended * params.v_g;
        s.q3 = blended * params.v_g;
        profile.push(s.interstitial_glucose(&params));
    }
    println!(
        "admit-10 with reanchor: {:?}",
        profile.iter().map(|v| format!("{v:.2}")).collect::<Vec<_>>().join(" ")
    );

    let state = belief_at_glucose(
        &params,
        &subject,
        mg_per_dl_to_mmol_per_l(180.0),
    );
    for k_agr in [0.5, 1.0, 5.0, 40.0] {
        let dose = nmpc_sequence_dose(
            &params,
            state,
            mg_per_dl_to_mmol_per_l(180.0),
            TARGET_GLUCOSE_MMOL_L,
            k_agr,
            20.0,
            subject.bir_u_per_h,
            240.0,
            15.0,
        );
        println!("engine-belief k_agr {k_agr}: dose {dose:.3}");
    }

    let w = moving_target_trajectory(mg_per_dl_to_mmol_per_l(180.0), TARGET_GLUCOSE_MMOL_L, 16, 15.0);
    let init = nmpc_grid_dose(&params, state, &w, subject.bir_u_per_h, 20.0, 5.0, 15.0);
    let seq = nmpc_sequence(
        &params, state, mg_per_dl_to_mmol_per_l(180.0), TARGET_GLUCOSE_MMOL_L, 5.0, 20.0,
        subject.bir_u_per_h, 240.0, 15.0,
    );
    let seq_cost = nmpc_sequence_cost(&params, state, &seq, &w, subject.bir_u_per_h, 5.0, 15.0);
    let init_const = nmpc_constant_cost(&params, state, init, &w, subject.bir_u_per_h, 5.0, 15.0);
    println!("grid init {init:.3} cost {init_const:.2}");
    println!("refined: {seq:?}  cost {seq_cost:.2}");
    for u in [0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 5.0, 10.0, 15.0, 20.0] {
        let mut seq = vec![0.0; 16];
        seq.fill(u);
        let c = nmpc_sequence_cost(&params, state, &seq, &w, subject.bir_u_per_h, 5.0, 15.0);

    if (u - 1.5).abs() < 1e-9 || (u - 0.0).abs() < 1e-9 {
        let mut s = state;
        let mut profile = Vec::new();
        for &r in &seq {
            s = s.step(&params, r, 0.0, 0.0, 15.0);
            profile.push(s.interstitial_glucose(&params));
        }
        println!("u {u:>4}: cost {c:.2} predicted-start {:.2} .. end {:.2}: {:?}", profile[0], profile[15], profile.iter().enumerate().map(|(i, v)| format!("{i}:{v:.1}")).collect::<String>());
    } else {
        println!("u {u:>4}: cost {c:.2}");
    }
    }

    for k_agr in [0.5, 1.0, 2.0, 3.0, 5.0, 8.0, 12.0, 20.0, 40.0] {
        let mut cfg = SimConfig::default();
        cfg.controller_kagr = k_agr;
        cfg.duration_hours = 24.0;
        cfg.admit_glucose_mg_per_dl = 140.0;
        cfg.max_delivery_u_per_h = 20.0;
        cfg.meals = vec![
            Meal { start_min: 360.0, carbs_g: 60.0, duration_min: 15.0 },
            Meal { start_min: 600.0, carbs_g: 50.0, duration_min: 15.0 },
            Meal { start_min: 840.0, carbs_g: 70.0, duration_min: 15.0 },
            Meal { start_min: 1140.0, carbs_g: 60.0, duration_min: 15.0 },
        ];
        let trace = simulate(&cfg);
        let (tir, low, mean) = metrics(&trace);
        let joined: Vec<String> = trace
            .delivered_u_per_h
            .iter()
            .enumerate()
            .filter(|(_, &r)| r > 1.6)
            .map(|(i, &r)| format!("t={}min:{r:.2}", i * 15))
            .collect();
        println!(
            "k_agr {k_agr:>5}: TIR {tir:6.1}%  low {low:5.1}%  mean {mean:5.2}  above-1.6 @ {}",
            joined.join(", ")
        );

        let mut desc = SimConfig::default();
        desc.controller_kagr = k_agr;
        desc.admit_glucose_mg_per_dl = 180.0;
        desc.duration_hours = 12.0;
        desc.max_delivery_u_per_h = 20.0;
        desc.pump_error_cv = 0.0;
        desc.meals = Vec::new();
        let trace = simulate(&desc);
        let first = trace.reading_mmol_per_l[0];
        let last = trace.reading_mmol_per_l.last().copied().unwrap();
        let peak = trace
            .delivered_u_per_h
            .iter()
            .cloned()
            .fold(0.0f64, f64::max);
        let mean_delivery = trace.delivered_u_per_h.iter().sum::<f64>() / trace.delivered_u_per_h.len() as f64;
        println!(
            "k_agr {k_agr:>5}: descent {first:.2} -> {last:.2}  peak {peak:.2}  mean-del {mean_delivery:.2}"
        );
    }
}