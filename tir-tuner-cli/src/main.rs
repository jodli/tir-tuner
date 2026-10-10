use tir_tuner_body::solver::with_basal_glucose;
use tir_tuner_cli::engine::{Meal, SimConfig, simulate};
use tir_tuner_common::data::BolusKind;
use tir_tuner_common::metrics::{
    coefficient_of_variation, mean_glucose, time_below_range_pct, time_in_range_pct,
};
use tir_tuner_common::units::{MgDl, mg_dl_series};

use chrono::NaiveDateTime;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tir_tuner_aps::controller::Nmpc;
use tir_tuner_aps::{AutoMode, TARGET_GLUCOSE_MMOL_L};
use tir_tuner_cli::settings_file;
use tir_tuner_store::{ProfileChanges, Store};

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
    /// The bolus calculator and Auto mode settings history
    #[command(subcommand)]
    Settings(SettingsCommand),
    /// Replay one Glooko export through the closed-loop simulation
    Simulate {
        dir: PathBuf,
        #[arg(long, default_value_t = tir_tuner_aps::controller::K_AGR)]
        kagr: f64,
    },
}

#[derive(Subcommand)]
enum SettingsCommand {
    /// Replace the stored history with the entries of a settings file
    Apply {
        #[arg(default_value = "settings.toml")]
        file: PathBuf,
    },
    /// Print the entry in force at a time (default: now), in file notation
    Show {
        /// e.g. 2026-10-05T23:00
        #[arg(long, value_parser = parse_datetime)]
        at: Option<NaiveDateTime>,
    },
}

fn parse_datetime(s: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .map_err(|_| format!("{s:?} is not like 2026-10-05T23:00"))
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
    #[error("{0}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("{0}")]
    SettingsFile(PathBuf, #[source] settings_file::Error),
    #[error("no settings stored for {0}")]
    NoSettings(NaiveDateTime),
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
        Command::Simulate { dir, kagr } => simulate_export(&dir, kagr, &cli.db),
        Command::Ingest { source, archive } => ingest(&source, &archive, &cli.db),
        Command::Report { .. } => Err(Error::NotImplemented("report")),
        Command::Recommend { .. } => Err(Error::NotImplemented("recommend")),
        Command::Settings(SettingsCommand::Apply { file }) => apply_settings(&file, &cli.db),
        Command::Settings(SettingsCommand::Show { at }) => show_settings(at, &cli.db),
    }
}

/// Attaches the database path to a store error.
fn in_db(db: &Path) -> impl Fn(tir_tuner_store::Error) -> Error + '_ {
    move |e| Error::Store(db.into(), e)
}

fn apply_settings(file: &Path, db: &Path) -> Result<(), Error> {
    let text = std::fs::read_to_string(file).map_err(|e| Error::Io(file.into(), e))?;
    let history = settings_file::parse(&text).map_err(|e| Error::SettingsFile(file.into(), e))?;
    let changes = Store::open(db)
        .and_then(|mut store| store.set_profiles(&history))
        .map_err(in_db(db))?;
    let list = |ts: &[NaiveDateTime]| {
        ts.iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    println!("{} entries in {}", history.entries().len(), file.display());
    if changes == ProfileChanges::default() {
        println!("stored settings unchanged");
    }
    for (what, ts) in [
        ("added", &changes.added),
        ("changed", &changes.changed),
        ("removed", &changes.removed),
    ] {
        if !ts.is_empty() {
            println!("{what}: {}", list(ts));
        }
    }
    Ok(())
}

fn show_settings(at: Option<NaiveDateTime>, db: &Path) -> Result<(), Error> {
    let at = at.unwrap_or_else(|| chrono::Local::now().naive_local());
    let history = Store::open(db)
        .and_then(|s| s.profiles())
        .map_err(in_db(db))?;
    let (from, profile) = history.at(at).ok_or(Error::NoSettings(at))?;
    print!("{}", settings_file::format(from, profile));
    Ok(())
}

fn ingest(source: &Path, archive: &Path, db: &Path) -> Result<(), Error> {
    let (dir, export) = tir_tuner_ingest::ingest(source, archive)?;
    let days = export
        .cgm
        .chunk_by(|a, b| a.at.date() == b.at.date())
        .count();
    let count = |k| export.boluses.iter().filter(|b| b.kind() == k).count();
    let (meals, hypo, corrections) = (
        count(BolusKind::Meal),
        count(BolusKind::HypoTreatment),
        count(BolusKind::Correction),
    );
    println!("archived to {}", dir.display());
    println!(
        "range {} .. {}, CGM on {days} days",
        export.start, export.end
    );
    println!(
        "{} CGM, {} fingerstick, {} boluses ({meals} meal, {hypo} hypo treatment, {corrections} correction), {} basal segments",
        export.cgm.len(),
        export.bg.len(),
        export.boluses.len(),
        export.basal.len(),
    );

    let store_err = in_db(db);
    let mut store = Store::open(db).map_err(&store_err)?;
    let new = store.insert(&export).map_err(&store_err)?;
    println!(
        "new in store: {} CGM, {} fingerstick, {} boluses, {} basal segments, {} alarms",
        new.cgm, new.bg, new.boluses, new.basal, new.alarms
    );
    if let Some((first, last)) = store.span().map_err(&store_err)? {
        println!("store covers {first} .. {last}");
    }
    Ok(())
}

fn simulate_export(dir: &Path, kagr: f64, db: &Path) -> Result<(), Error> {
    let export = tir_tuner_ingest::read_export(dir)?;
    let cgm = &export.cgm;
    let real: Vec<MgDl> = cgm.iter().map(|r| MgDl(r.mg_dl)).collect();
    print_metrics("real Glooko", &real);

    // Everything eaten, hypo treatments included, relative to the CGM start.
    let cgm_start = cgm.first().ok_or_else(|| Error::NoCgm(dir.into()))?.at;
    let meals: Vec<Meal> = export
        .boluses
        .iter()
        .filter(|b| b.kind() != BolusKind::Correction)
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
        TARGET_GLUCOSE_MMOL_L,
    );
    let history = Store::open(db)
        .and_then(|s| s.profiles())
        .map_err(in_db(db))?;
    let auto = match history.at(cgm_start) {
        Some((_, p)) => AutoMode::new(p.auto()),
        None => {
            println!("no settings stored for {cgm_start}, using the default target and weight");
            AutoMode::without_personal_target(subject.weight_kg)
        }
    };
    println!(
        "Auto mode: target {:.1} mmol/L at start, weight {} kg",
        auto.target_mmol_l(cgm_start.time()),
        auto.weight_kg()
    );
    let cfg = SimConfig {
        subject,
        auto,
        start_time: cgm_start.time(),
        admit_glucose_mg_per_dl: 140.0,
        duration_hours: 24.0,
        meals,
        nmpc: Nmpc {
            k_agr: kagr,
            ..Nmpc::default()
        },
        ..SimConfig::default()
    };
    let trace = simulate(&cfg);

    print_metrics(
        "sim (closed loop)",
        &mg_dl_series(&trace.reading_mmol_per_l),
    );
    Ok(())
}

fn print_metrics(label: &str, readings: &[MgDl]) {
    println!(
        "{label}: {} samples, TIR {:.1}%, TBR {:.1}%, mean {:.0} mg/dL, CV {:.1}%",
        readings.len(),
        time_in_range_pct(readings),
        time_below_range_pct(readings),
        mean_glucose(readings).0,
        coefficient_of_variation(readings),
    );
}
