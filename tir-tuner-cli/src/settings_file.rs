//! `settings.toml`: the settings history in the app's block notation.
//!
//! ```toml
//! [[profile]]
//! from = 2026-09-09T00:00:00
//! insulin_action_hours = 2.5
//! max_bolus_u = 30
//! min_glucose_mg_dl = 70
//! weight_kg = 50
//! total_daily_dose_u = 36
//! carb_ratio = { "00-06" = 17, "06-11" = 12, "11-17" = 13, "17-24" = 11 }
//! correction_factor = { "06-22" = 100, "22-06" = 80 }
//! bolus_target = { "06-22" = 110, "22-06" = 120 }
//! # auto_target: optional, defaults as in `ProfileInput::auto_target`
//! ```

use std::collections::BTreeMap;

use chrono::{NaiveDate, NaiveDateTime};
use serde::Deserialize;
use tir_tuner_common::settings::{
    Block, DuplicateEntry, Profile, ProfileHistory, ProfileInput, Setting, SettingsError,
};

/// Why the file can't be read.
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum Error {
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error("{0} is not a local date and time")]
    From(toml::value::Datetime),
    #[error("entry from {0}: {1}: {2:?} is not a block like \"06-11\" or \"06:30-11:00\"")]
    Key(NaiveDateTime, Setting, String),
    #[error("entry from {0}")]
    Settings(NaiveDateTime, #[source] SettingsError),
    #[error(transparent)]
    Duplicate(#[from] DuplicateEntry),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    profile: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    from: toml::value::Datetime,
    insulin_action_hours: f64,
    max_bolus_u: f64,
    min_glucose_mg_dl: f64,
    weight_kg: f64,
    total_daily_dose_u: f64,
    carb_ratio: BTreeMap<String, f64>,
    correction_factor: BTreeMap<String, f64>,
    bolus_target: BTreeMap<String, f64>,
    auto_target: Option<BTreeMap<String, f64>>,
}

/// The file's entries as a settings history.
pub fn parse(text: &str) -> Result<ProfileHistory, Error> {
    let file: File = toml::from_str(text)?;
    let entries = file
        .profile
        .into_iter()
        .map(entry)
        .collect::<Result<_, _>>()?;
    Ok(ProfileHistory::new(entries)?)
}

fn entry(e: Entry) -> Result<(NaiveDateTime, Profile), Error> {
    let from = local_datetime(&e.from).ok_or(Error::From(e.from))?;
    let blocks = |setting, map: &BTreeMap<String, f64>| {
        map.iter()
            .map(|(key, &value)| {
                let (start_min, end_min) =
                    key_minutes(key).ok_or_else(|| Error::Key(from, setting, key.clone()))?;
                Ok(Block {
                    start_min,
                    end_min,
                    value,
                })
            })
            .collect::<Result<Vec<_>, Error>>()
    };
    let input = ProfileInput {
        carb_ratio: blocks(Setting::CarbRatio, &e.carb_ratio)?,
        correction_factor: blocks(Setting::CorrectionFactor, &e.correction_factor)?,
        bolus_target: blocks(Setting::BolusTarget, &e.bolus_target)?,
        auto_target: e
            .auto_target
            .as_ref()
            .map(|m| blocks(Setting::AutoTarget, m))
            .transpose()?,
        insulin_action_hours: e.insulin_action_hours,
        max_bolus_u: e.max_bolus_u,
        min_glucose_mg_dl: e.min_glucose_mg_dl,
        weight_kg: e.weight_kg,
        total_daily_dose_u: e.total_daily_dose_u,
    };
    let profile = Profile::new(input).map_err(|err| Error::Settings(from, err))?;
    Ok((from, profile))
}

fn local_datetime(d: &toml::value::Datetime) -> Option<NaiveDateTime> {
    let (date, time) = (d.date?, d.time?);
    if d.offset.is_some() {
        return None;
    }
    NaiveDate::from_ymd_opt(date.year.into(), date.month.into(), date.day.into())?.and_hms_opt(
        time.hour.into(),
        time.minute.into(),
        time.second.unwrap_or(0).into(),
    )
}

/// `"06-11"`, `"22-06"`, `"06:30-11:00"`; `24` ends the day.
fn key_minutes(key: &str) -> Option<(u16, u16)> {
    let time = |s: &str| -> Option<u16> {
        let (h, m) = s.split_once(':').unwrap_or((s, "0"));
        let min = h.parse::<u16>().ok()? * 60 + m.parse::<u16>().ok().filter(|m| *m < 60)?;
        (min <= 24 * 60).then_some(min)
    };
    let (a, b) = key.split_once('-')?;
    Some((time(a)?, time(b)?))
}

/// One entry in the file's notation, so `settings show` can be pasted back.
pub fn format(from: NaiveDateTime, p: &Profile) -> String {
    let i = p.to_input();
    let mut out = format!(
        "[[profile]]\nfrom = {}\ninsulin_action_hours = {}\nmax_bolus_u = {}\n\
         min_glucose_mg_dl = {}\nweight_kg = {}\ntotal_daily_dose_u = {}\n",
        from.format("%Y-%m-%dT%H:%M:%S"),
        i.insulin_action_hours,
        i.max_bolus_u,
        i.min_glucose_mg_dl,
        i.weight_kg,
        i.total_daily_dose_u,
    );
    let schedules = [
        (Setting::CarbRatio, Some(&i.carb_ratio)),
        (Setting::CorrectionFactor, Some(&i.correction_factor)),
        (Setting::BolusTarget, Some(&i.bolus_target)),
        (Setting::AutoTarget, i.auto_target.as_ref()),
    ];
    for (setting, blocks) in schedules {
        let Some(blocks) = blocks else { continue };
        let blocks: Vec<String> = blocks
            .iter()
            .map(|b| format!("\"{}\" = {}", b.label(), b.value))
            .collect();
        out += &format!("{setting} = {{ {} }}\n", blocks.join(", "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/settings.toml");

    #[test]
    fn show_output_reads_back() {
        let history = parse(FIXTURE).unwrap();
        assert_eq!(history.entries().len(), 2);
        let printed: String = history
            .entries()
            .iter()
            .map(|(f, p)| format(*f, p) + "\n")
            .collect();
        assert_eq!(parse(&printed).unwrap(), history);
    }

    #[test]
    fn errors_name_the_entry_and_block() {
        let text = FIXTURE.replacen("\"06-11\"", "\"07-11\"", 1);
        let err = parse(&text).unwrap_err();
        let chain = format!("{err}: {}", std::error::Error::source(&err).unwrap());
        assert_eq!(
            chain,
            "entry from 2026-09-09 00:00:00: carb_ratio: nothing set at 06"
        );
    }
}
