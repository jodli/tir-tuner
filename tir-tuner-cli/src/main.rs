use tir_tuner_body::solver::with_basal_glucose;
use tir_tuner_cli::engine::{simulate, Meal, SimConfig};
use tir_tuner_cli::glooko;
use tir_tuner_common::metrics::{
    coefficient_of_variation, mean_glucose, time_in_range_pct,
};
use tir_tuner_common::units::mg_per_dl_to_mmol_per_l;

fn main() {
    if let Err(e) = run() {
        eprintln!("tir-tuner-cli: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let ingest = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/Users/jan-olaf.becker/repos/personal/tir-tuner/ingest/2026-08-05".into());
    let dir = std::path::Path::new(&ingest);

    let cgm_path = dir.join("cgm_data_1.csv");
    let bolus_path = dir.join("Insulin data/bolus_data_1.csv");
    if !cgm_path.exists() {
        return Err(format!("no CGM export at {}", cgm_path.display()).into());
    }

    let cgm_text = std::fs::read_to_string(&cgm_path)?;
    let cgm = glooko::parse_cgm(&cgm_text)?;
    let real_mmol: Vec<f64> = cgm.iter().map(|p| mg_per_dl_to_mmol_per_l(p.mg_per_dl)).collect();
    println!(
        "real Glooko: {} samples, TIR {:.1}%, mean {:.2} mmol/L, CV {:.1}%",
        cgm.len(),
        time_in_range_pct(&real_mmol),
        mean_glucose(&real_mmol),
        coefficient_of_variation(&real_mmol),
    );

    // Build a meal plan from the bolus export if present.
    let mut meals = Vec::new();
    if let Ok(text) = std::fs::read_to_string(&bolus_path) {
        let bolus = glooko::parse_meals(&text)?;
        // Absolute minutes to relative minutes from the CGM start.
        let cgm_start = cgm.first().map(|p| p.t_min).unwrap_or(0);
        for m in bolus {
            let rel = (m.t_min - cgm_start) as f64;
            meals.push(Meal {
                start_min: rel,
                carbs_g: m.carbs_g,
                duration_min: 15.0,
            });
        }
        println!("meals from bolus export: {} ({} in-sim)", meals.len(), meals.iter().filter(|m| m.start_min >= 0.0 && m.start_min <= 24.0 * 60.0).count());
    }

    // In-silico run over 24 hours starting at a representative admit.
    let subject = with_basal_glucose(&tir_tuner_body::subject::VirtualSubject::population_mean(), 5.8);
    let cfg = SimConfig {
        subject,
        admit_glucose_mg_per_dl: 140.0,
        duration_hours: 24.0,
        meals,
        max_delivery_u_per_h: 20.0,
        ..SimConfig::default()
    };
    let trace = simulate(&cfg);

    let sim_readings = &trace.reading_mmol_per_l;
    println!(
        "sim (closed loop): {} samples, TIR {:.1}%, mean {:.2} mmol/L, CV {:.1}%",
        sim_readings.len(),
        time_in_range_pct(sim_readings),
        mean_glucose(sim_readings),
        coefficient_of_variation(sim_readings),
    );

    Ok(())
}