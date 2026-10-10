//! Glooko CSV, English or German: banner line, header line, rows newest first.
//! German quotes decimal commas (`"77,0"`) and writes `DD.MM.YYYY HH:MM`.

use chrono::{NaiveDate, NaiveDateTime};
use csv::StringRecord;
use tir_tuner_common::data::{Alarm, BasalSegment, Bolus, GlucoseReading};

// Header names, English then German.
const TIME: &[&str] = &["Timestamp", "Zeitstempel"];
const CGM: &[&str] = &["CGM Glucose Value (mg/dl)", "CGM-Glukosewert (mg/dl)"];
const BG: &[&str] = &["Glucose Value (mg/dl)", "Glukosewert (mg/dl)"];
const CARBS: &[&str] = &["Carbs Input (g)", "Kohlenhydrataufnahme (g)"];
const DELIVERED: &[&str] = &["Insulin Delivered (U)", "Abgegebenes Insulin (E)"];
const DURATION: &[&str] = &["Duration (minutes)", "Dauer (Minuten)"];
const RATE: &[&str] = &["Rate"];
const ALARM: &[&str] = &["Alarm/Event", "Alarm/Ereignis"];
// German alarm names as English ones; unknown names are kept as they are.
const ALARM_NAMES: &[(&str, &str)] = &[
    ("Okklusion", "Occlusion"),
    ("Batterie schwach", "Low Battery"),
];

/// Daily totals are derivable and the manual tables are empty, so they're skipped.
#[derive(Clone, Copy)]
pub(crate) enum Table {
    Cgm,
    Bg,
    Bolus,
    Basal,
    Alarm,
}

impl Table {
    /// `<table>_data_<n>.csv`; Glooko spills tables over 20000 rows into `n` > 1.
    pub(crate) fn from_file_name(name: &str) -> Option<Self> {
        let (prefix, n) = name.strip_suffix(".csv")?.rsplit_once("_data_")?;
        n.parse::<u32>().ok()?;
        match prefix {
            "cgm" => Some(Self::Cgm),
            "bg" => Some(Self::Bg),
            "bolus" => Some(Self::Bolus),
            "basal" => Some(Self::Basal),
            "alarms" => Some(Self::Alarm),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(crate) struct Rows {
    pub cgm: Vec<GlucoseReading>,
    pub bg: Vec<GlucoseReading>,
    pub boluses: Vec<Bolus>,
    pub basal: Vec<BasalSegment>,
    pub alarms: Vec<Alarm>,
}

#[derive(Debug, thiserror::Error)]
#[error("line {line}: {msg}")]
pub struct FormatError {
    pub line: u64,
    pub msg: String,
}

fn err(line: u64, msg: impl Into<String>) -> FormatError {
    FormatError {
        line,
        msg: msg.into(),
    }
}

enum Cols {
    Glucose(usize),
    Bolus { carbs: usize, units: usize },
    Basal { minutes: usize, rate: usize },
    Alarm(usize),
}

/// Appends one file's rows and returns its banner date range.
pub(crate) fn parse(
    table: Table,
    text: &str,
    rows: &mut Rows,
) -> Result<(NaiveDate, NaiveDate), FormatError> {
    let mut records = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.trim_start_matches('\u{feff}').as_bytes())
        .into_records();
    let banner = record(records.next())?.ok_or_else(|| err(1, "empty file"))?;
    let range = banner
        .iter()
        .find_map(banner_range)
        .ok_or_else(|| err(1, "no date range in banner"))?;
    let header = record(records.next())?.ok_or_else(|| err(2, "no header"))?;
    let col = |names: &[&str]| {
        header
            .iter()
            .position(|h| names.contains(&h))
            .ok_or_else(|| err(2, format!("missing column {:?}", names[0])))
    };
    let time = col(TIME)?;
    let cols = match table {
        Table::Cgm => Cols::Glucose(col(CGM)?),
        Table::Bg => Cols::Glucose(col(BG)?),
        Table::Bolus => Cols::Bolus {
            carbs: col(CARBS)?,
            units: col(DELIVERED)?,
        },
        Table::Basal => Cols::Basal {
            minutes: col(DURATION)?,
            rate: col(RATE)?,
        },
        Table::Alarm => Cols::Alarm(col(ALARM)?),
    };

    while let Some(r) = record(records.next())? {
        let line = r.position().map_or(0, |p| p.line());
        let field = |i: usize| r.get(i).unwrap_or("").trim();
        let need = |i: usize| {
            number(field(i)).ok_or_else(|| err(line, format!("bad number {:?}", field(i))))
        };
        let at = timestamp(field(time))
            .ok_or_else(|| err(line, format!("bad timestamp {:?}", field(time))))?;
        match cols {
            Cols::Glucose(v) => {
                let reading = GlucoseReading {
                    at,
                    mg_dl: need(v)?,
                };
                match table {
                    Table::Cgm => rows.cgm.push(reading),
                    _ => rows.bg.push(reading),
                }
            }
            Cols::Bolus { carbs, units } => rows.boluses.push(Bolus {
                at,
                carbs_g: number(field(carbs)).unwrap_or(0.0),
                units: need(units)?,
            }),
            Cols::Basal { minutes, rate } => rows.basal.push(BasalSegment {
                at,
                minutes: need(minutes)? as u32,
                rate_u_h: need(rate)?,
            }),
            Cols::Alarm(kind) => {
                let name = field(kind);
                let kind = ALARM_NAMES
                    .iter()
                    .find(|(de, _)| *de == name)
                    .map_or(name, |(_, en)| en);
                rows.alarms.push(Alarm {
                    at,
                    kind: kind.to_string(),
                });
            }
        }
    }
    Ok(range)
}

fn record(
    r: Option<Result<StringRecord, csv::Error>>,
) -> Result<Option<StringRecord>, FormatError> {
    r.transpose()
        .map_err(|e| err(e.position().map_or(0, |p| p.line()), e.to_string()))
}

/// `Date Range:2026-09-24 - 2026-10-07` or `Datumsbereich:24.09.2026 - 07.10.2026`.
fn banner_range(field: &str) -> Option<(NaiveDate, NaiveDate)> {
    let value = field
        .strip_prefix("Date Range:")
        .or_else(|| field.strip_prefix("Datumsbereich:"))?;
    let (a, b) = value.split_once(" - ")?;
    let date = |s: &str| {
        let s = s.trim();
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .or_else(|_| NaiveDate::parse_from_str(s, "%d.%m.%Y"))
            .ok()
    };
    Some((date(a)?, date(b)?))
}

fn timestamp(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%d.%m.%Y %H:%M"))
        .ok()
}

/// Empty and `-` mean no value. No thousands separators in Glooko exports.
fn number(s: &str) -> Option<f64> {
    if s.is_empty() || s == "-" {
        return None;
    }
    s.replace(',', ".").parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn german_bolus() {
        let text = "Name:REDACTED,Datumsbereich:22.07.2026 - 05.08.2026
Zeitstempel,Insulin-Typ,Blutzuckereingabe (mg/dl),Kohlenhydrataufnahme (g),Kohlenhydratverhältnis,Abgegebenes Insulin (E),Anfängliche Abgabe (E),Verzögerte Abgabe (E),Seriennummer
31.07.2026 19:31,Normal,\"0,0\",\"50,0\",,\"4,5\",,,CamAPS mylife YpsoPump
31.07.2026 16:02,Normal,\"0,0\",\"0,0\",,\"1,5\",,,CamAPS mylife YpsoPump
";
        let mut rows = Rows::default();
        let range = parse(Table::Bolus, text, &mut rows).unwrap();
        assert_eq!(range.1, NaiveDate::from_ymd_opt(2026, 8, 5).unwrap());
        assert_eq!(rows.boluses[0].carbs_g, 50.0);
        assert_eq!(rows.boluses[0].units, 4.5);
        assert!(!rows.boluses[1].is_meal());
        assert_eq!(rows.boluses[1].at.to_string(), "2026-07-31 16:02:00");
    }

    #[test]
    fn english_basal() {
        let text = "Name:REDACTED,Date Range:2026-09-24 - 2026-10-07
Timestamp,Insulin Type,Duration (minutes),Percentage (%),Rate,Insulin Delivered (U),Serial Number
2026-10-07 18:08,Scheduled,73,,0.0,,CamAPS mylife YpsoPump
2026-10-07 18:00,Scheduled,8,,0.51,,CamAPS mylife YpsoPump
";
        let mut rows = Rows::default();
        parse(Table::Basal, text, &mut rows).unwrap();
        assert_eq!(rows.basal[1].minutes, 8);
        assert_eq!(rows.basal[1].rate_u_h, 0.51);
    }
}
