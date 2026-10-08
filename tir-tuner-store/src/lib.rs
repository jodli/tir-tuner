//! Event store: all ingested exports in one DuckDB file, deduplicated per row.

mod migrate;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use chrono::{Days, NaiveDate, NaiveDateTime, NaiveTime};
use duckdb::{Appender, Connection, params};
use refinery::Runner;
use tir_tuner_common::data::{BasalSegment, Bolus, Export, GlucoseReading};

use migrate::Duck;

refinery::embed_migrations!("migrations");

pub type Error = Box<dyn std::error::Error>;

/// New rows per table from one insert.
#[derive(Debug, Default, PartialEq)]
pub struct Inserted {
    pub cgm: usize,
    pub bg: usize,
    pub boluses: usize,
    pub basal: usize,
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
            std::fs::copy(path, backup_path(path, current))?;
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
