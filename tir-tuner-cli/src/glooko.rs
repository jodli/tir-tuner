//! Glooko CSV export parser.
//!
//! Glooko's German exports are the opposite of RFC 4180: `,` is the
//! decimal separator and quote-delimited, the timestamp is
//! `DD.MM.YYYY HH:MM`, and CGM rows come in reverse chronological
//! order. This parser only extracts what the simulation needs: the CGM
//! glucose series and the meal (carbohydrate) events from the bolus
//! table. Everything else in the export is ignored.
//!
//! The files start with two header lines
//! (`Name:REDACTED,...` and `Zeitstempel,...`); rows after that are
//! data. Both separators and quotes are tokenized line by line.

use std::error::Error;
use std::fmt;

static CGM_HEADER: &str = "Zeitstempel,CGM-Glukosewert (mg/dl)";
static BOLUS_HEADER: &str = "Zeitstempel,Insulin-Typ,Blutzuckereingabe (mg/dl),Kohlenhydrataufnahme (g)";

/// One CGM sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CgmPoint {
    /// Minutes since a fixed epoch (2000-01-01 00:00 UTC), for sorting.
    pub t_min: i64,
    /// Glucose in mg/dL.
    pub mg_per_dl: f64,
}

/// One meal event from the bolus table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MealEvent {
    /// Minutes since the fixed epoch.
    pub t_min: i64,
    /// Carbohydrates in grams.
    pub carbs_g: f64,
}

/// Split one CSV data row honoring quoted fields (Glooko quotes decimal
/// commas, so a bare split on `,` would tear `"50,0"` in half).
fn split_row(row: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for c in row.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    fields.push(cur);
    fields
}

/// Parse a German decimal (either `,` or `.`) into an f64. Tolerates a
/// surrounding pair of double quotes.
fn parse_german_decimal(s: &str) -> Option<f64> {
    let s = s.trim();
    let s = s.strip_prefix('"').unwrap_or(s).strip_suffix('"').unwrap_or(s);
    if s.is_empty() || s == "-" {
        return None;
    }
    let normalized = s.replace(',', ".");
    normalized.parse::<f64>().ok()
}

/// `DD.MM.YYYY HH:MM` -> minutes since 2000-01-01 00:00.
fn parse_timestamp(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, time) = s.split_once(' ')?;
    let mut dp = date.split('.');
    let day: i64 = dp.next()?.parse().ok()?;
    let month: i64 = dp.next()?.parse().ok()?;
    let year: i64 = dp.next()?.parse().ok()?;
    let (hour, minute) = time.split_once(':')?;
    let hour: i64 = hour.parse().ok()?;
    let minute: i64 = minute.parse().ok()?;

    let epoch_year = 2000;
    // Days in preceding full years and months; Gregorian.
    let leap = |y: i64| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut days = 0;
    for y in epoch_year..year {
        days += if leap(y) { 366 } else { 365 };
    }
    for (i, &dim) in days_in_month.iter().enumerate() {
        if i + 1 < month as usize {
            days += dim;
            if i + 1 == 2 && leap(year) {
                days += 1;
            }
        }
    }
    days += day - 1;
    Some((days * 1440) + hour * 60 + minute)
}

/// Parse a CGM export: returns the samples in ascending time order.
pub fn parse_cgm(contents: &str) -> Result<Vec<CgmPoint>, ParseError> {
    let mut out = Vec::new();
    let mut in_header = true;
    for (line_no, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if in_header {
            if line.starts_with("Name:") || line.starts_with(CGM_HEADER) {
                continue;
            }
            in_header = false;
        }
        let fields = split_row(line);
        if fields.len() < 2 {
            continue;
        }
        let t = parse_timestamp(&fields[0]).ok_or_else(|| ParseError::row(line_no, "timestamp", line))?;
        let v = parse_german_decimal(&fields[1]).ok_or_else(|| ParseError::row(line_no, "glucose", line))?;
        out.push(CgmPoint { t_min: t, mg_per_dl: v });
    }
    out.sort_by_key(|p| p.t_min);
    Ok(out)
}

/// Parse the bolus export: meal (carbohydrate) events in ascending time.
pub fn parse_meals(contents: &str) -> Result<Vec<MealEvent>, ParseError> {
    let mut out = Vec::new();
    let mut in_header = true;
    for (line_no, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if in_header {
            if line.starts_with("Name:") || line.starts_with(BOLUS_HEADER) {
                continue;
            }
            in_header = false;
        }
        let fields = split_row(line);
        if fields.len() < 4 {
            continue;
        }
        let t = parse_timestamp(&fields[0]).ok_or_else(|| ParseError::row(line_no, "timestamp", line))?;
        // Carrier column: Kohlenhydrataufnahme (g). Zero/empty means no
        // meal bolus; skip those (a bolus without carbs is corrective).
        if let Some(carbs) = parse_german_decimal(&fields[3]) {
            if carbs > 0.0 {
                out.push(MealEvent { t_min: t, carbs_g: carbs });
            }
        }
    }
    out.sort_by_key(|p| p.t_min);
    Ok(out)
}

/// A row that could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line number in the source CSV.
    pub line: usize,
    /// The field being parsed when the row failed.
    pub field: &'static str,
    /// The unparsed row text.
    pub row: String,
}

impl ParseError {
    fn row(line: usize, field: &'static str, row: &str) -> Self {
        Self { line, field, row: row.to_string() }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: could not parse {} in {:?}", self.line, self.field, self.row)
    }
}

impl Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    const CGM: &str = "Name:REDACTED,Datumsbereich:22.07.2026 - 05.08.2026
Zeitstempel,CGM-Glukosewert (mg/dl),Seriennummer
05.08.2026 23:59,\"137,0\",CamAPS FreeStyle Libre 3
05.08.2026 23:58,\"136,0\",CamAPS FreeStyle Libre 3
05.08.2026 23:57,\"136,0\",CamAPS FreeStyle Libre 3
";
    const BOLUS: &str = "Name:REDACTED,Datumsbereich:22.07.2026 - 05.08.2026
Zeitstempel,Insulin-Typ,Blutzuckereingabe (mg/dl),Kohlenhydrataufnahme (g),Kohlenhydratverhältnis,Abgegebenes Insulin (E),Anfängliche Abgabe (E),Verzögerte Abgabe (E),Seriennummer
31.07.2026 19:31,Normal,\"0,0\",\"50,0\",,4,5,,CamAPS mylife YpsoPump
31.07.2026 00:01,Normal,\"0,0\",\"10,0\",,0,0,,CamAPS mylife YpsoPump
31.07.2026 06:36,Normal,\"0,0\",\"45,0\",,4,1,,CamAPS mylife YpsoPump
";

    #[test]
    fn parses_german_decimal() {
        assert_eq!(parse_german_decimal("\"137,0\""), Some(137.0));
        assert_eq!(parse_german_decimal("50,0"), Some(50.0));
        assert_eq!(parse_german_decimal("31.5"), Some(31.5));
        assert_eq!(parse_german_decimal("-"), None);
        assert_eq!(parse_german_decimal(""), None);
    }

    #[test]
    fn parses_timestamp() {
        // 2000-01-01 00:00 -> 0. 2000 is a leap year.
        assert_eq!(parse_timestamp("01.01.2000 00:00"), Some(0));
        assert_eq!(parse_timestamp("01.01.2000 00:05"), Some(5));
        // Days: Jan has 31 days, so Feb 1 00:00 = 31*1440.
        assert_eq!(parse_timestamp("01.02.2000 00:00"), Some(31 * 1440));
    }

    #[test]
    fn cgm_is_ascending_with_values() {
        let pts = parse_cgm(CGM).unwrap();
        assert_eq!(pts.len(), 3);
        // Reverse chronological input, so ascending order reverses it.
        assert!(pts[0].t_min < pts[1].t_min && pts[1].t_min < pts[2].t_min);
        assert_eq!(pts[0].mg_per_dl, 136.0);
        assert_eq!(pts[2].mg_per_dl, 137.0);
    }

    #[test]
    fn bolus_extracts_only_positive_carbs() {
        let meals = parse_meals(BOLUS).unwrap();
        // Three rows, all with carbs; ascending time order.
        assert_eq!(meals.len(), 3);
        assert_eq!(meals[0].carbs_g, 10.0);
        assert_eq!(meals[1].carbs_g, 45.0);
        assert_eq!(meals[2].carbs_g, 50.0);
        assert!(meals[0].t_min < meals[1].t_min);
    }

    #[test]
    fn empty_input_gives_empty_result() {
        assert!(parse_cgm("").unwrap().is_empty());
        assert!(parse_meals("Name:REDACTED,whatever\nZeitstempel,a,b\n").unwrap().is_empty());
    }
}