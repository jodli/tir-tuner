//! Therapy settings as entered in the CamAPS FX app, checked against the
//! ranges the app allows (user manual, PDF pages 29, 59-64).
//!
//! [`ProfileInput`] is what a frontend fills in; [`Profile::new`] is the only
//! way to a [`Profile`], so the settings the algorithms read are always valid.
#![allow(missing_docs)]

use std::fmt;

use chrono::{NaiveDateTime, NaiveTime, Timelike};

use crate::units::{mg_per_dl_to_mmol_per_l, mmol_per_l_to_mg_per_dl};

/// The app sets every time block in 30 min steps.
pub const STEP_MIN: u16 = 30;
const DAY_MIN: u16 = 24 * 60;
const SLOTS: usize = (DAY_MIN / STEP_MIN) as usize;

/// Auto mode target where no personal target is set (p59).
pub const DEFAULT_TARGET_MMOL_L: f64 = 5.8;

/// A setting, named as in `settings.toml` and the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    CarbRatio,
    CorrectionFactor,
    BolusTarget,
    AutoTarget,
    InsulinActionHours,
    MaxBolus,
    MinGlucose,
    Weight,
    TotalDailyDose,
}

impl Setting {
    /// The settings that are time blocks.
    pub const SCHEDULES: [Setting; 4] = [
        Self::CarbRatio,
        Self::CorrectionFactor,
        Self::BolusTarget,
        Self::AutoTarget,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::CarbRatio => "carb_ratio",
            Self::CorrectionFactor => "correction_factor",
            Self::BolusTarget => "bolus_target",
            Self::AutoTarget => "auto_target",
            Self::InsulinActionHours => "insulin_action_hours",
            Self::MaxBolus => "max_bolus_u",
            Self::MinGlucose => "min_glucose_mg_dl",
            Self::Weight => "weight_kg",
            Self::TotalDailyDose => "total_daily_dose_u",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::SCHEDULES.into_iter().find(|s| s.name() == name)
    }
}

impl fmt::Display for Setting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
#[error("{setting}: {msg}")]
pub struct SettingsError {
    pub setting: Setting,
    pub msg: String,
}

/// One time block. `end_min` is exclusive and may be before `start_min` for
/// a block across midnight (`22-06`); the whole day is `0..1440`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub start_min: u16,
    pub end_min: u16,
    pub value: f64,
}

impl Block {
    pub fn all_day(value: f64) -> Self {
        Self {
            start_min: 0,
            end_min: DAY_MIN,
            value,
        }
    }

    /// `06-22`, `06:30-22`, as the app and `settings.toml` write blocks.
    pub fn label(&self) -> String {
        format!("{}-{}", clock(self.start_min), clock(self.end_min))
    }

    fn slots(&self) -> impl Iterator<Item = usize> {
        let (start, end) = (self.start_min / STEP_MIN, self.end_min / STEP_MIN);
        let len = (end as usize + SLOTS - start as usize - 1) % SLOTS + 1;
        (0..len).map(move |k| (start as usize + k) % SLOTS)
    }
}

/// A value per time of day, covering all 24 hours.
#[derive(Clone, Debug, PartialEq)]
pub struct Schedule {
    by_slot: [f64; SLOTS],
}

impl Schedule {
    /// Blocks on the 30 min grid, no overlaps; gaps get `default` or fail.
    fn build(blocks: &[Block], default: Option<f64>) -> Result<Self, String> {
        let mut by_slot = [f64::NAN; SLOTS];
        for b in blocks {
            let on_grid = |m: u16| m % STEP_MIN == 0 && m <= DAY_MIN;
            if !on_grid(b.start_min) || !on_grid(b.end_min) || b.start_min == DAY_MIN {
                return Err(format!("{} is not on the 30 min grid", b.label()));
            }
            if !(b.value.is_finite() && b.value > 0.0) {
                return Err(format!("{} must be positive", b.label()));
            }
            for i in b.slots() {
                if !by_slot[i].is_nan() {
                    return Err(format!("{} overlaps another block", b.label()));
                }
                by_slot[i] = b.value;
            }
        }
        for (i, slot) in by_slot.iter_mut().enumerate().filter(|(_, v)| v.is_nan()) {
            *slot =
                default.ok_or_else(|| format!("nothing set at {}", clock(i as u16 * STEP_MIN)))?;
        }
        Ok(Self { by_slot })
    }

    pub fn at(&self, t: NaiveTime) -> f64 {
        self.by_slot[(t.hour() * 60 + t.minute()) as usize / STEP_MIN as usize]
    }

    /// Fewest blocks: runs of equal values, the one across midnight whole.
    pub fn blocks(&self) -> Vec<Block> {
        let s = &self.by_slot;
        let Some(origin) = (0..SLOTS).find(|&i| s[i] != s[(i + SLOTS - 1) % SLOTS]) else {
            return vec![Block::all_day(s[0])];
        };
        let mut blocks: Vec<Block> = Vec::new();
        for i in (origin..origin + SLOTS).map(|i| i % SLOTS) {
            let start_min = i as u16 * STEP_MIN;
            let end_min = start_min + STEP_MIN;
            match blocks.last_mut() {
                Some(b) if b.value == s[i] => b.end_min = end_min,
                _ => blocks.push(Block {
                    start_min,
                    end_min,
                    value: s[i],
                }),
            }
        }
        for b in &mut blocks {
            // Back to clock minutes: a block past midnight ends at 00:00..24:00.
            if b.end_min > DAY_MIN {
                b.end_min -= DAY_MIN;
            }
        }
        blocks.sort_by_key(|b| b.start_min);
        blocks
    }
}

/// `06`, `06:30`.
fn clock(min: u16) -> String {
    match min % 60 {
        0 => format!("{:02}", min / 60),
        m => format!("{:02}:{m:02}", min / 60),
    }
}

/// Settings as entered in the app, unchecked.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileInput {
    /// g carbs per unit.
    pub carb_ratio: Vec<Block>,
    /// mg/dl drop per unit.
    pub correction_factor: Vec<Block>,
    /// mg/dl.
    pub bolus_target: Vec<Block>,
    /// Personal target, mg/dl. `None`: same as the bolus target. Times
    /// without a block use [`DEFAULT_TARGET_MMOL_L`].
    pub auto_target: Option<Vec<Block>>,
    pub insulin_action_hours: f64,
    pub max_bolus_u: f64,
    /// Below this the calculator refuses to suggest a bolus.
    pub min_glucose_mg_dl: f64,
    pub weight_kg: f64,
    /// Average of the last 5 days, Auto mode's initial sensitivity estimate.
    pub total_daily_dose_u: f64,
}

/// The bolus calculator. None of it affects Auto mode (p62).
#[derive(Clone, Debug, PartialEq)]
pub struct BolusSettings {
    carb_ratio: Schedule,
    correction_factor: Schedule,
    target: Schedule,
    insulin_action_hours: f64,
    max_bolus_u: f64,
    min_glucose_mg_dl: f64,
}

impl BolusSettings {
    pub fn carb_ratio(&self) -> &Schedule {
        &self.carb_ratio
    }
    pub fn correction_factor(&self) -> &Schedule {
        &self.correction_factor
    }
    pub fn target(&self) -> &Schedule {
        &self.target
    }
    pub fn insulin_action_hours(&self) -> f64 {
        self.insulin_action_hours
    }
    pub fn max_bolus_u(&self) -> f64 {
        self.max_bolus_u
    }
    pub fn min_glucose_mg_dl(&self) -> f64 {
        self.min_glucose_mg_dl
    }
}

/// Auto mode inputs (p29, p59, p70).
#[derive(Clone, Debug, PartialEq)]
pub struct AutoSettings {
    target: Schedule,
    weight_kg: f64,
    total_daily_dose_u: f64,
}

impl AutoSettings {
    pub fn target(&self) -> &Schedule {
        &self.target
    }
    pub fn weight_kg(&self) -> f64 {
        self.weight_kg
    }
    pub fn total_daily_dose_u(&self) -> f64 {
        self.total_daily_dose_u
    }
}

/// Bolus calculator and Auto mode settings that passed the app's checks.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    bolus: BolusSettings,
    auto: AutoSettings,
}

impl Profile {
    pub fn new(input: ProfileInput) -> Result<Self, SettingsError> {
        use Setting::*;
        let fail = |setting, msg| SettingsError { setting, msg };
        let schedule = |setting, blocks: &[Block], default| {
            Schedule::build(blocks, default).map_err(|msg| fail(setting, msg))
        };
        let range = |setting, v: f64, (lo, hi): (f64, f64)| {
            if (lo..=hi).contains(&v) {
                Ok(())
            } else {
                Err(fail(setting, format!("{v} is outside {lo}..{hi}")))
            }
        };
        // The manual gives glucose ranges in mmol/L; the settings are mg/dl.
        let glucose = |setting, what: String, mg_dl: f64, (lo, hi): (f64, f64)| {
            if (lo..=hi).contains(&mg_per_dl_to_mmol_per_l(mg_dl)) {
                Ok(())
            } else {
                Err(fail(
                    setting,
                    format!("{what}{mg_dl} mg/dl is outside {lo}..{hi} mmol/L"),
                ))
            }
        };

        let bolus_target = schedule(BolusTarget, &input.bolus_target, None)?;
        for b in &input.bolus_target {
            glucose(
                BolusTarget,
                format!("{} = ", b.label()),
                b.value,
                (4.4, 11.0),
            )?;
        }
        let auto_target = match &input.auto_target {
            Some(blocks) => {
                let default = mmol_per_l_to_mg_per_dl(DEFAULT_TARGET_MMOL_L);
                schedule(AutoTarget, blocks, Some(default))?
            }
            None => bolus_target.clone(),
        };
        glucose(
            MinGlucose,
            String::new(),
            input.min_glucose_mg_dl,
            (2.8, 4.4),
        )?;
        range(InsulinActionHours, input.insulin_action_hours, (2.0, 8.0))?;
        range(MaxBolus, input.max_bolus_u, (0.5, 30.0))?;
        if (input.max_bolus_u * 2.0).fract() != 0.0 {
            return Err(fail(MaxBolus, "must be a multiple of 0.5".into()));
        }
        range(Weight, input.weight_kg, (10.0, 300.0))?;
        range(TotalDailyDose, input.total_daily_dose_u, (5.0, 350.0))?;

        Ok(Self {
            bolus: BolusSettings {
                carb_ratio: schedule(CarbRatio, &input.carb_ratio, None)?,
                correction_factor: schedule(CorrectionFactor, &input.correction_factor, None)?,
                target: bolus_target,
                insulin_action_hours: input.insulin_action_hours,
                max_bolus_u: input.max_bolus_u,
                min_glucose_mg_dl: input.min_glucose_mg_dl,
            },
            auto: AutoSettings {
                target: auto_target,
                weight_kg: input.weight_kg,
                total_daily_dose_u: input.total_daily_dose_u,
            },
        })
    }

    pub fn bolus(&self) -> &BolusSettings {
        &self.bolus
    }

    pub fn auto(&self) -> &AutoSettings {
        &self.auto
    }

    /// The four time-block settings, in [`Setting::SCHEDULES`] order.
    pub fn schedules(&self) -> [(Setting, &Schedule); 4] {
        let b = &self.bolus;
        Setting::SCHEDULES.map(|s| {
            let schedule = match s {
                Setting::CarbRatio => &b.carb_ratio,
                Setting::CorrectionFactor => &b.correction_factor,
                Setting::BolusTarget => &b.target,
                _ => &self.auto.target,
            };
            (s, schedule)
        })
    }

    /// The fewest blocks that give this profile back from [`Profile::new`].
    pub fn to_input(&self) -> ProfileInput {
        let (b, a) = (&self.bolus, &self.auto);
        ProfileInput {
            carb_ratio: b.carb_ratio.blocks(),
            correction_factor: b.correction_factor.blocks(),
            bolus_target: b.target.blocks(),
            auto_target: (a.target != b.target).then(|| a.target.blocks()),
            insulin_action_hours: b.insulin_action_hours,
            max_bolus_u: b.max_bolus_u,
            min_glucose_mg_dl: b.min_glucose_mg_dl,
            weight_kg: a.weight_kg,
            total_daily_dose_u: a.total_daily_dose_u,
        }
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
#[error("two settings entries start at {0}")]
pub struct DuplicateEntry(pub NaiveDateTime);

/// Settings over time: each entry is in force from its start until the next.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfileHistory {
    entries: Vec<(NaiveDateTime, Profile)>,
}

impl ProfileHistory {
    pub fn new(mut entries: Vec<(NaiveDateTime, Profile)>) -> Result<Self, DuplicateEntry> {
        entries.sort_by_key(|(from, _)| *from);
        if let Some(w) = entries.windows(2).find(|w| w[0].0 == w[1].0) {
            return Err(DuplicateEntry(w[0].0));
        }
        Ok(Self { entries })
    }

    /// The entry in force at `t`, with its start.
    pub fn at(&self, t: NaiveDateTime) -> Option<(NaiveDateTime, &Profile)> {
        let n = self.entries.partition_point(|(from, _)| *from <= t);
        n.checked_sub(1)
            .map(|i| (self.entries[i].0, &self.entries[i].1))
    }

    /// Oldest first.
    pub fn entries(&self) -> &[(NaiveDateTime, Profile)] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(start_h: u16, end_h: u16, value: f64) -> Block {
        Block {
            start_min: start_h * 60,
            end_min: end_h * 60,
            value,
        }
    }

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn block_across_midnight() {
        let cf = Schedule::build(&[block(6, 22, 100.0), block(22, 6, 80.0)], None).unwrap();
        assert_eq!(cf.at(t(5, 59)), 80.0);
        assert_eq!(cf.at(t(6, 0)), 100.0);
        assert_eq!(cf.at(t(23, 0)), 80.0);
        assert_eq!(cf.blocks(), vec![block(6, 22, 100.0), block(22, 6, 80.0)]);
    }

    #[test]
    fn overlap_names_the_block() {
        let err = Schedule::build(&[block(0, 12, 17.0), block(11, 24, 12.0)], None);
        assert_eq!(err.unwrap_err(), "11-24 overlaps another block");
    }

    #[test]
    fn auto_target_gaps_get_the_default() {
        let target = Schedule::build(&[block(6, 20, 117.0)], Some(104.5)).unwrap();
        assert_eq!(target.at(t(5, 30)), 104.5);
        assert_eq!(
            target.blocks(),
            vec![block(6, 20, 117.0), block(20, 6, 104.5)]
        );
    }

    #[test]
    fn history_picks_the_entry_in_force() {
        let day = |d: u32| {
            chrono::NaiveDate::from_ymd_opt(2026, 9, d)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
        };
        let input = |cr| ProfileInput {
            carb_ratio: vec![Block::all_day(cr)],
            correction_factor: vec![Block::all_day(100.0)],
            bolus_target: vec![Block::all_day(110.0)],
            auto_target: None,
            insulin_action_hours: 2.5,
            max_bolus_u: 30.0,
            min_glucose_mg_dl: 70.0,
            weight_kg: 50.0,
            total_daily_dose_u: 36.0,
        };
        let (a, b) = (
            Profile::new(input(12.0)).unwrap(),
            Profile::new(input(11.0)).unwrap(),
        );
        let h = ProfileHistory::new(vec![(day(20), b.clone()), (day(9), a.clone())]).unwrap();
        assert_eq!(h.at(day(8)), None);
        assert_eq!(h.at(day(19)), Some((day(9), &a)));
        assert_eq!(h.at(day(20)), Some((day(20), &b)));
        assert_eq!(
            ProfileHistory::new(vec![(day(9), a.clone()), (day(9), b)]),
            Err(DuplicateEntry(day(9)))
        );
    }
}
