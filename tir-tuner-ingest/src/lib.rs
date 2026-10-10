//! Reads Glooko exports into [`Export`]s and archives redacted copies.

mod glooko;
mod redact;

use std::io::Read;
use std::path::{Path, PathBuf};

use tir_tuner_common::data::{Export, GlucoseReading};

pub use glooko::FormatError;
use glooko::{Rows, Table};

/// Why an export could not be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("{0}: not a readable zip")]
    Zip(PathBuf, #[source] zip::result::ZipError),
    #[error("{0}")]
    Format(PathBuf, #[source] FormatError),
    #[error("{0}: no Glooko export found")]
    NoData(PathBuf),
    /// Files in one export cover different date ranges, so it's several exports.
    #[error("{0}: files from more than one export")]
    MixedExports(PathBuf),
}

/// CSV files of one export, paths relative to its root.
type Files = Vec<(PathBuf, String)>;

/// Reads a raw export (zip or directory), archives it with the name redacted
/// to `archive/<end-date>/` and returns that directory with the parsed export.
pub fn ingest(source: &Path, archive: &Path) -> Result<(PathBuf, Export), Error> {
    let files = if source.is_dir() {
        read_dir(source)?
    } else {
        read_zip(source)?
    };
    let export = parse(source, &files)?;
    let target = archive.join(export.end.to_string());
    for (rel, text) in files {
        let path = target.join(rel);
        let parent = path.parent().unwrap_or(&target);
        std::fs::create_dir_all(parent).map_err(|e| Error::Io(parent.into(), e))?;
        std::fs::write(&path, redact::redact(&text)).map_err(|e| Error::Io(path, e))?;
    }
    Ok((target, export))
}

/// Reads one export directory.
pub fn read_export(dir: &Path) -> Result<Export, Error> {
    parse(dir, &read_dir(dir)?)
}

fn parse(source: &Path, files: &Files) -> Result<Export, Error> {
    let mut rows = Rows::default();
    let mut range = None;
    for (rel, text) in files {
        let name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let Some(table) = Table::from_file_name(name) else {
            continue;
        };
        let r = glooko::parse(table, text, &mut rows)
            .map_err(|e| Error::Format(source.join(rel), e))?;
        if range.is_some_and(|known| known != r) {
            return Err(Error::MixedExports(source.into()));
        }
        range = Some(r);
    }
    let (start, end) = range.ok_or_else(|| Error::NoData(source.into()))?;

    // Exports sometimes repeat a row verbatim.
    rows.bg.sort_by_key(|r| r.at);
    rows.bg.dedup();
    rows.boluses.sort_by(|a, b| {
        (a.at.cmp(&b.at))
            .then(a.carbs_g.total_cmp(&b.carbs_g))
            .then(a.units.total_cmp(&b.units))
    });
    rows.boluses.dedup();
    rows.basal.sort_by(|a, b| {
        (a.at, a.minutes)
            .cmp(&(b.at, b.minutes))
            .then(a.rate_u_h.total_cmp(&b.rate_u_h))
    });
    rows.basal.dedup();
    rows.alarms
        .sort_by(|a, b| (a.at, &a.kind).cmp(&(b.at, &b.kind)));
    rows.alarms.dedup();
    Ok(Export {
        start,
        end,
        cgm: merge_same_minute(rows.cgm),
        bg: rows.bg,
        boluses: rows.boluses,
        basal: rows.basal,
        alarms: rows.alarms,
    })
}

/// Libre 3 sometimes logs a minute twice, with slightly different values.
fn merge_same_minute(mut cgm: Vec<GlucoseReading>) -> Vec<GlucoseReading> {
    cgm.sort_by_key(|r| r.at);
    cgm.chunk_by(|a, b| a.at == b.at)
        .map(|g| GlucoseReading {
            at: g[0].at,
            mg_dl: g.iter().map(|r| r.mg_dl).sum::<f64>() / g.len() as f64,
        })
        .collect()
}

/// Every `.csv` under `dir`, any depth (bolus and basal sit in `Insulin data/`).
fn read_dir(dir: &Path) -> Result<Files, Error> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).map_err(|e| Error::Io(d.clone(), e))? {
            let path = entry.map_err(|e| Error::Io(d.clone(), e))?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "csv") {
                let text =
                    std::fs::read_to_string(&path).map_err(|e| Error::Io(path.clone(), e))?;
                out.push((path.strip_prefix(dir).unwrap_or(&path).to_path_buf(), text));
            }
        }
    }
    Ok(out)
}

fn read_zip(path: &Path) -> Result<Files, Error> {
    let io = |e: std::io::Error| Error::Io(path.into(), e);
    let file = std::fs::File::open(path).map_err(io)?;
    let zip_err = |e| Error::Zip(path.into(), e);
    let mut zip = zip::ZipArchive::new(file).map_err(zip_err)?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(zip_err)?;
        // enclosed_name rejects paths escaping the target dir.
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        if rel.extension().is_none_or(|e| e != "csv") || rel.starts_with("__MACOSX") {
            continue;
        }
        let mut text = String::new();
        entry.read_to_string(&mut text).map_err(io)?;
        out.push((rel, text));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spill_files_merge_and_duplicate_minutes_average() {
        let head = "Name:REDACTED,Date Range:2026-09-24 - 2026-10-07\nTimestamp,CGM Glucose Value (mg/dl),Serial Number\n";
        let files: Files = vec![
            (
                "cgm_data_1.csv".into(),
                format!("{head}2026-09-28 15:31,220.0,x\n2026-09-28 15:31,211.0,x\n"),
            ),
            (
                "cgm_data_2.csv".into(),
                format!("{head}2026-09-24 00:00,100.0,x\n"),
            ),
            (
                "cgm_carbs_data_1.csv".into(),
                "Name:REDACTED,Date Range:2026-09-24 - 2026-10-07\nTimestamp,Carbs (g)\n".into(),
            ),
        ];
        let export = parse(Path::new("x"), &files).unwrap();
        assert_eq!(export.cgm.len(), 2);
        assert_eq!(export.cgm[0].mg_dl, 100.0);
        assert_eq!(export.cgm[1].mg_dl, 215.5);
    }
}
