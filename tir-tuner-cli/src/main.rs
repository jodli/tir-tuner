use tir_tuner_body::solver::with_basal_glucose;
use tir_tuner_cli::engine::{Meal, SimConfig, simulate};
use tir_tuner_common::metrics::{coefficient_of_variation, mean_glucose, time_in_range_pct};
use tir_tuner_common::units::mg_per_dl_to_mmol_per_l;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tir_tuner_store::Store;

#[derive(Parser)]
#[command(about = "Tunes carb ratio and correction factor from Glooko exports")]
struct Cli {
    /// The event store
    #[arg(long, global = true, default_value = "tir-tuner.duckdb")]
    db: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Redact a Glooko export (zip or directory), archive it and read it
    Ingest {
        source: PathBuf,
        /// Where redacted exports are kept, one directory per end date
        #[arg(long, default_value = "ingest")]
        archive: PathBuf,
    },
    /// Glucose metrics over the last N days
    Report {
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
    /// CR/CF proposals from the last N days against the baseline
    Recommend {
        #[arg(long, default_value_t = 3)]
        recent: u32,
    },
    /// Manage the configured CR/CF schedule
    Settings,
    /// Replay one Glooko export through the closed-loop simulation
    Simulate {
        dir: PathBuf,
        #[arg(long, default_value_t = tir_tuner_cli::engine::CONTROLLER_KAGR_DEFAULT)]
        kagr: f64,
    },
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Ingest(#[from] tir_tuner_ingest::Error),
    #[error("{0}")]
    Store(PathBuf, #[source] tir_tuner_store::Error),
    #[error("{0}: not implemented")]
    NotImplemented(&'static str),
    #[error("{0}: no CGM data")]
    NoCgm(PathBuf),
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        let mut msg = e.to_string();
        let mut cause = std::error::Error::source(&e);
        while let Some(c) = cause {
            msg += &format!(": {c}");
            cause = c.source();
        }
        eprintln!("tir-tuner-cli: {msg}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Error> {
    match cli.command {
        Command::Simulate { dir, kagr } => simulate_export(&dir, kagr),
        Command::Ingest { source, archive } => ingest(&source, &archive, &cli.db),
        Command::Report { .. } => Err(Error::NotImplemented("report")),
        Command::Recommend { .. } => Err(Error::NotImplemented("recommend")),
        Command::Settings => Err(Error::NotImplemented("settings")),
    }
}

fn ingest(source: &Path, archive: &Path, db: &Path) -> Result<(), Error> {
    let (dir, export) = tir_tuner_ingest::ingest(source, archive)?;
    let days = export
        .cgm
        .chunk_by(|a, b| a.at.date() == b.at.date())
        .count();
    let meals = export.boluses.iter().filter(|b| b.is_meal()).count();
    println!("archived to {}", dir.display());
    println!(
        "range {} .. {}, CGM on {days} days",
        export.start, export.end
    );
    println!(
        "{} CGM, {} fingerstick, {} boluses ({meals} meal, {} correction), {} basal segments",
        export.cgm.len(),
        export.bg.len(),
        export.boluses.len(),
        export.boluses.len() - meals,
        export.basal.len(),
    );

    let store_err = |e| Error::Store(db.into(), e);
    let mut store = Store::open(db).map_err(store_err)?;
    let new = store.insert(&export).map_err(store_err)?;
    println!(
        "new in store: {} CGM, {} fingerstick, {} boluses, {} basal segments",
        new.cgm, new.bg, new.boluses, new.basal
    );
    if let Some((first, last)) = store.span().map_err(store_err)? {
        println!("store covers {first} .. {last}");
    }
    Ok(())
}

fn simulate_export(dir: &Path, kagr: f64) -> Result<(), Error> {
    let export = tir_tuner_ingest::read_export(dir)?;
    let cgm = &export.cgm;
    let real_mmol: Vec<f64> = cgm
        .iter()
        .map(|r| mg_per_dl_to_mmol_per_l(r.mg_dl))
        .collect();
    println!(
        "real Glooko: {} samples, TIR {:.1}%, mean {:.2} mmol/L, CV {:.1}%",
        cgm.len(),
        time_in_range_pct(&real_mmol),
        mean_glucose(&real_mmol),
        coefficient_of_variation(&real_mmol),
    );

    // Meals relative to the CGM start.
    let cgm_start = cgm.first().ok_or_else(|| Error::NoCgm(dir.into()))?.at;
    let meals: Vec<Meal> = export
        .boluses
        .iter()
        .filter(|b| b.is_meal())
        .map(|b| Meal {
            start_min: (b.at - cgm_start).num_minutes() as f64,
            carbs_g: b.carbs_g,
            duration_min: 15.0,
        })
        .collect();
    println!(
        "meals from bolus export: {} ({} in-sim)",
        meals.len(),
        meals
            .iter()
            .filter(|m| m.start_min >= 0.0 && m.start_min <= 24.0 * 60.0)
            .count()
    );

    // In-silico run over 24 hours starting at a representative admit.
    let subject = with_basal_glucose(
        &tir_tuner_body::subject::VirtualSubject::population_mean(),
        5.8,
    );
    let cfg = SimConfig {
        subject,
        admit_glucose_mg_per_dl: 140.0,
        duration_hours: 24.0,
        meals,
        max_delivery_u_per_h: 20.0,
        controller_kagr: kagr,
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
