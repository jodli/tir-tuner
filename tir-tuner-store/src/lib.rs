//! Event store: all ingested exports in one DuckDB file, deduplicated per row.

mod migrate;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{Days, NaiveDate, NaiveDateTime, NaiveTime};
use duckdb::{Appender, Connection, params};
use refinery::Runner;
use tir_tuner_common::data::{Alarm, BasalSegment, Bolus, Export, GlucoseReading};
use tir_tuner_common::settings::{
    Block, Profile, ProfileHistory, ProfileInput, Setting, SettingsError,
};

use migrate::Duck;

refinery::embed_migrations!("migrations");

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Db(#[from] duckdb::Error),
    #[error("migration failed")]
    Migration(#[from] refinery::Error),
    #[error("backup to {0} failed")]
    Backup(PathBuf, #[source] std::io::Error),
    #[error("stored settings from {0} are invalid")]
    StoredProfile(NaiveDateTime, #[source] SettingsError),
}

/// Settings entries `set_profiles` added, changed or removed, by start time.
#[derive(Debug, Default, PartialEq)]
pub struct ProfileChanges {
    pub added: Vec<NaiveDateTime>,
    pub changed: Vec<NaiveDateTime>,
    pub removed: Vec<NaiveDateTime>,
}

/// New rows per table from one insert.
#[derive(Debug, Default, PartialEq)]
pub struct Inserted {
    pub cgm: usize,
    pub bg: usize,
    pub boluses: usize,
    pub basal: usize,
    pub alarms: usize,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens or creates the store. Copies the file to `<path>.bak-v<n>` before
    /// applying pending migrations.
    pub fn open(path: &Path) -> Result<Self, Error> {
        Self::open_with(Connection::open(path)?, Some(path), migrations::runner())
    }

    #[cfg(test)]
    fn open_in_memory() -> Result<Self, Error> {
        Self::open_with(Connection::open_in_memory()?, None, migrations::runner())
    }

    fn open_with(mut conn: Connection, path: Option<&Path>, runner: Runner) -> Result<Self, Error> {
        let current = schema_version(&conn)?;
        let latest = runner
            .get_migrations()
            .iter()
            .map(|m| m.version())
            .max()
            .unwrap_or(0);
        if let Some(path) = path
            && current > 0
            && current < latest
        {
            conn.execute_batch("CHECKPOINT")?;
            let backup = backup_path(path, current);
            std::fs::copy(path, &backup).map_err(|e| Error::Backup(backup, e))?;
        }
        // Grouped: all pending migrations commit together or not at all.
        runner.set_grouped(true).run(&mut Duck(&mut conn))?;
        Ok(Self { conn })
    }

    /// Adds rows not stored yet; exports never change old rows.
    pub fn insert(&mut self, export: &Export) -> Result<Inserted, Error> {
        let tx = self.conn.transaction()?;
        let n = Inserted {
            cgm: stage(&tx, "cgm", &export.cgm)?,
            bg: stage(&tx, "bg", &export.bg)?,
            boluses: stage(&tx, "bolus", &export.boluses)?,
            basal: stage(&tx, "basal", &export.basal)?,
            alarms: stage(&tx, "alarm", &export.alarms)?,
        };
        tx.commit()?;
        Ok(n)
    }

    /// Everything recorded on the days `from..=to`.
    pub fn range(&self, from: NaiveDate, to: NaiveDate) -> Result<Export, Error> {
        let lo = from.and_time(NaiveTime::MIN);
        let hi = (to + Days::new(1)).and_time(NaiveTime::MIN);
        Ok(Export {
            start: from,
            end: to,
            cgm: fetch(&self.conn, "cgm", lo, hi)?,
            bg: fetch(&self.conn, "bg", lo, hi)?,
            boluses: fetch(&self.conn, "bolus", lo, hi)?,
            basal: fetch(&self.conn, "basal", lo, hi)?,
            alarms: fetch(&self.conn, "alarm", lo, hi)?,
        })
    }

    /// First and last day with CGM data.
    pub fn span(&self) -> Result<Option<(NaiveDate, NaiveDate)>, Error> {
        let (lo, hi): (Option<NaiveDateTime>, Option<NaiveDateTime>) =
            self.conn
                .query_row("SELECT min(ts), max(ts) FROM cgm", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
        Ok(lo.zip(hi).map(|(a, b)| (a.date(), b.date())))
    }

    /// Replaces the whole settings history.
    pub fn set_profiles(&mut self, history: &ProfileHistory) -> Result<ProfileChanges, Error> {
        let mut old: BTreeMap<_, _> = self.profiles()?.entries().iter().cloned().collect();
        let mut changes = ProfileChanges::default();
        for (from, p) in history.entries() {
            match old.remove(from) {
                None => changes.added.push(*from),
                Some(o) if o != *p => changes.changed.push(*from),
                Some(_) => {}
            }
        }
        changes.removed = old.into_keys().collect();

        let tx = self.conn.transaction()?;
        tx.execute_batch("DELETE FROM profile_block; DELETE FROM profile;")?;
        let mut profile = tx.prepare(
            "INSERT INTO profile (effective_from, insulin_action_hours, max_bolus_u, \
             min_glucose_mg_dl, weight_kg, total_daily_dose_u) VALUES (?, ?, ?, ?, ?, ?)",
        )?;
        let mut block = tx.prepare("INSERT INTO profile_block VALUES (?, ?, ?, ?, ?)")?;
        for (from, p) in history.entries() {
            let (b, a) = (p.bolus(), p.auto());
            profile.execute(params![
                from,
                b.insulin_action_hours(),
                b.max_bolus_u(),
                b.min_glucose_mg_dl(),
                a.weight_kg(),
                a.total_daily_dose_u()
            ])?;
            for (setting, schedule) in p.schedules() {
                for bl in schedule.blocks() {
                    block.execute(params![
                        from,
                        setting.name(),
                        bl.start_min,
                        bl.end_min,
                        bl.value
                    ])?;
                }
            }
        }
        drop((profile, block));
        tx.commit()?;
        Ok(changes)
    }

    pub fn profiles(&self) -> Result<ProfileHistory, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT effective_from, setting, start_min, end_min, value FROM profile_block",
        )?;
        let mut blocks: BTreeMap<(NaiveDateTime, String), Vec<Block>> = BTreeMap::new();
        let rows = stmt.query_map([], |r| {
            let block = Block {
                start_min: r.get(2)?,
                end_min: r.get(3)?,
                value: r.get(4)?,
            };
            Ok(((r.get(0)?, r.get(1)?), block))
        })?;
        for row in rows {
            let (key, block) = row?;
            blocks.entry(key).or_default().push(block);
        }
        let mut take = |from: NaiveDateTime, s: Setting| {
            blocks
                .remove(&(from, s.name().to_string()))
                .unwrap_or_default()
        };

        let mut stmt = self.conn.prepare(
            "SELECT effective_from, insulin_action_hours, max_bolus_u, min_glucose_mg_dl, \
             weight_kg, total_daily_dose_u FROM profile",
        )?;
        let rows: Vec<(NaiveDateTime, [f64; 5])> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    [r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?],
                ))
            })?
            .collect::<duckdb::Result<_>>()?;
        let mut entries = Vec::new();
        for (from, [dia, max_bolus, min_glucose, weight, tdd]) in rows {
            let input = ProfileInput {
                carb_ratio: take(from, Setting::CarbRatio),
                correction_factor: take(from, Setting::CorrectionFactor),
                bolus_target: take(from, Setting::BolusTarget),
                auto_target: Some(take(from, Setting::AutoTarget)),
                insulin_action_hours: dia,
                max_bolus_u: max_bolus,
                min_glucose_mg_dl: min_glucose,
                weight_kg: weight,
                total_daily_dose_u: tdd,
            };
            let p = Profile::new(input).map_err(|e| Error::StoredProfile(from, e))?;
            entries.push((from, p));
        }
        // effective_from is the primary key, so no two entries share a start.
        Ok(ProfileHistory::new(entries).expect("unique effective_from"))
    }
}

/// Maps a domain type to its table columns, in table order after `ts`.
trait Row: Sized {
    const COLS: &'static str;
    fn append(&self, a: &mut Appender) -> duckdb::Result<()>;
    fn read(r: &duckdb::Row) -> duckdb::Result<Self>;
}

impl Row for GlucoseReading {
    const COLS: &'static str = "mg_dl";
    fn append(&self, a: &mut Appender) -> duckdb::Result<()> {
        a.append_row(params![self.at, self.mg_dl])
    }
    fn read(r: &duckdb::Row) -> duckdb::Result<Self> {
        Ok(Self {
            at: r.get(0)?,
            mg_dl: r.get(1)?,
        })
    }
}

impl Row for Bolus {
    const COLS: &'static str = "carbs_g, units";
    fn append(&self, a: &mut Appender) -> duckdb::Result<()> {
        a.append_row(params![self.at, self.carbs_g, self.units])
    }
    fn read(r: &duckdb::Row) -> duckdb::Result<Self> {
        Ok(Self {
            at: r.get(0)?,
            carbs_g: r.get(1)?,
            units: r.get(2)?,
        })
    }
}

impl Row for BasalSegment {
    const COLS: &'static str = "minutes, rate_u_h";
    fn append(&self, a: &mut Appender) -> duckdb::Result<()> {
        a.append_row(params![self.at, self.minutes, self.rate_u_h])
    }
    fn read(r: &duckdb::Row) -> duckdb::Result<Self> {
        Ok(Self {
            at: r.get(0)?,
            minutes: r.get(1)?,
            rate_u_h: r.get(2)?,
        })
    }
}

impl Row for Alarm {
    const COLS: &'static str = "kind";
    fn append(&self, a: &mut Appender) -> duckdb::Result<()> {
        a.append_row(params![self.at, self.kind])
    }
    fn read(r: &duckdb::Row) -> duckdb::Result<Self> {
        Ok(Self {
            at: r.get(0)?,
            kind: r.get(1)?,
        })
    }
}

/// Bulk-loads `rows` into a temp copy of `table`, then adds the ones `table`
/// doesn't have. Row-by-row inserts take ~10 s for a 30-day export.
fn stage<T: Row>(conn: &Connection, table: &str, rows: &[T]) -> duckdb::Result<usize> {
    conn.execute_batch(&format!(
        "CREATE OR REPLACE TEMP TABLE staged AS FROM {table} LIMIT 0"
    ))?;
    let mut appender = conn.appender_to_catalog_and_db("staged", "temp", "main")?;
    for r in rows {
        r.append(&mut appender)?;
    }
    appender.flush()?;
    conn.execute(&format!("INSERT OR IGNORE INTO {table} FROM staged"), [])
}

fn fetch<T: Row>(
    conn: &Connection,
    table: &str,
    lo: NaiveDateTime,
    hi: NaiveDateTime,
) -> duckdb::Result<Vec<T>> {
    let cols = T::COLS;
    let mut stmt = conn.prepare(&format!(
        "SELECT ts, {cols} FROM {table} WHERE ts >= ? AND ts < ? ORDER BY ts, {cols}"
    ))?;
    stmt.query_map(params![lo, hi], T::read)?.collect()
}

/// Highest applied migration, 0 for a new file.
fn schema_version(conn: &Connection) -> duckdb::Result<i32> {
    let exists: bool = conn.query_row(
        "SELECT count(*) > 0 FROM information_schema.tables WHERE table_name = 'refinery_schema_history'",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(0);
    }
    conn.query_row(
        "SELECT max(version) FROM refinery_schema_history",
        [],
        |r| r.get(0),
    )
}

fn backup_path(path: &Path, version: i32) -> PathBuf {
    path.with_added_extension(format!("bak-v{version}"))
}
