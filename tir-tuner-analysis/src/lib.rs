//! Meal and correction episodes from recorded data, judged against the
//! settings in force at the time.
//!
//! A meal is judged at the end of the insulin action time: by then the
//! glucose should be back in range. A rise after that is a late rise
//! (fatty meals), which the carb ratio doesn't explain.
#![allow(missing_docs)]

use std::cmp::Ordering;

use chrono::{NaiveDateTime, TimeDelta};
use tir_tuner_bolus::round_to_step;
use tir_tuner_common::data::{Bolus, BolusKind, Export, GlucoseReading};
use tir_tuner_common::metrics::{above_range, below_range, in_range};
use tir_tuner_common::settings::{Block, Profile, ProfileHistory, walsh_basal_u_per_h};
use tir_tuner_common::units::{Grams, GramsPerUnit, MIN_PER_HOUR, MgDl, MgDlPerUnit, Units};

/// Boluses this close are one meal: the user splits a meal and stops when
/// full. src: user-split-meal
const MERGE_MIN: i64 = 30;
/// Fatty meals rise again 4-5 h after the bolus. src: user-late-rise
const LATE_END_MIN: i64 = 360;
/// A longer CGM gap leaves an episode unreadable. src: own-cgm-gap
const MAX_GAP_MIN: i64 = 20;
/// An occlusion this long before may still leave the meal under-dosed.
/// src: own-occlusion-lookback
const OCCLUSION_LOOKBACK_MIN: i64 = 240;
/// The ingest's name for it. src: glooko-occlusion
const OCCLUSION: &str = "Occlusion";
/// src: math-median
const HALF: usize = 2;
/// Neighbouring readings. src: math-median
const PAIR: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dose {
    /// carbs / carb ratio, on the pump's step.
    AsCalculated,
    WithCorrection,
    LessThanCalculated,
}

/// Why an episode isn't judged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exclusion {
    CgmGap,
    /// Another meal before the action time is over.
    NextMeal(NaiveDateTime),
    /// The previous meal's insulin still acts.
    PreviousMeal(NaiveDateTime),
    /// Carbs while a correction acts.
    Carbs(NaiveDateTime),
    /// Another correction while one acts.
    Stacked(NaiveDateTime),
    Occlusion(NaiveDateTime),
    SettingsChange(NaiveDateTime),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outcome {
    pub start: MgDl,
    pub peak: MgDl,
    pub peak_after: TimeDelta,
    pub lowest: MgDl,
    pub at_action_end: MgDl,
    /// Correction boluses after the meal, before the action end.
    pub corrections: Units,
    pub hypo_carbs: Grams,
    /// Auto mode insulin above the resting basal; negative when it backed off.
    pub auto_extra: Units,
    /// Above range, then below: insulin too late for the carbs, not too much.
    pub spike_then_low: bool,
    /// In range at the action end, above range later. `None` when the late
    /// window can't be read (gap, next meal).
    pub late_rise: Option<bool>,
}

impl Outcome {
    pub fn in_range_at_end(&self) -> bool {
        in_range(self.at_action_end)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Meal {
    pub at: NaiveDateTime,
    pub boluses: usize,
    pub carbs: Grams,
    pub units: Units,
    /// The carb ratio block in force.
    pub block: Block,
    pub calculated: Units,
    pub dose: Dose,
    pub outcome: Result<Outcome, Exclusion>,
}

impl Meal {
    /// Carbs over all the insulin the meal ended up with by the action end.
    pub fn effective_carb_ratio(&self) -> Option<GramsPerUnit> {
        let o = self.outcome.as_ref().ok()?;
        let insulin = self.units + o.corrections + o.auto_extra;
        Some(GramsPerUnit(self.carbs.0 / insulin.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CorrectionOutcome {
    pub start: MgDl,
    pub at_action_end: MgDl,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Correction {
    pub at: NaiveDateTime,
    pub units: Units,
    /// The correction factor block in force.
    pub block: Block,
    pub outcome: Result<CorrectionOutcome, Exclusion>,
}

impl Correction {
    /// Observed drop per unit. Auto mode backs off after a correction, so
    /// this overstates the correction factor's own effect.
    pub fn drop_per_unit(&self) -> Option<MgDlPerUnit> {
        let o = self.outcome.as_ref().ok()?;
        Some(MgDlPerUnit((o.start - o.at_action_end).0 / self.units.0))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analysis {
    pub meals: Vec<Meal>,
    pub corrections: Vec<Correction>,
    /// Boluses before the first settings entry.
    pub without_settings: usize,
}

pub fn analyse(export: &Export, history: &ProfileHistory) -> Analysis {
    let mut out = Analysis::default();
    let meals = meal_groups(export);
    for (k, group) in meals.iter().enumerate() {
        let at = group[0].at;
        let Some((from, profile)) = history.at(at) else {
            out.without_settings += group.len();
            continue;
        };
        let s = profile.bolus();
        let carbs = Grams(group.iter().map(|b| b.carbs_g).sum());
        let units = Units(group.iter().map(|b| b.units).sum());
        let calculated = round_to_step(carbs / s.carb_ratio(at.time()));
        let dose = match round_to_step(units).0.total_cmp(&calculated.0) {
            Ordering::Equal => Dose::AsCalculated,
            Ordering::Greater => Dose::WithCorrection,
            Ordering::Less => Dose::LessThanCalculated,
        };
        let next_meal = meals.get(k + 1).map(|g| g[0].at);
        let previous_meal = k.checked_sub(1).and_then(|j| meals[j].last()).map(|b| b.at);
        out.meals.push(Meal {
            at,
            boluses: group.len(),
            carbs,
            units,
            block: s.carb_ratios().block_at(at.time()),
            calculated,
            dose,
            outcome: meal_outcome(
                export,
                history,
                (from, profile),
                at,
                previous_meal,
                next_meal,
            ),
        });
    }
    for b in export
        .boluses
        .iter()
        .filter(|b| b.kind() == BolusKind::Correction)
    {
        let Some((from, profile)) = history.at(b.at) else {
            out.without_settings += 1;
            continue;
        };
        out.corrections.push(Correction {
            at: b.at,
            units: Units(b.units),
            block: profile.bolus().correction_factors().block_at(b.at.time()),
            outcome: correction_outcome(export, history, (from, profile), b),
        });
    }
    out
}

/// Meal boluses, each group one meal.
fn meal_groups(export: &Export) -> Vec<Vec<&Bolus>> {
    let mut groups: Vec<Vec<&Bolus>> = Vec::new();
    let merge = TimeDelta::minutes(MERGE_MIN);
    for b in export
        .boluses
        .iter()
        .filter(|b| b.kind() == BolusKind::Meal)
    {
        match groups.last_mut() {
            Some(g) if g.last().is_some_and(|last| b.at - last.at <= merge) => g.push(b),
            _ => groups.push(vec![b]),
        }
    }
    groups
}

/// Glucose should be in range again by then. src: user-in-range-at-action-end
fn action_end(profile: &Profile, at: NaiveDateTime) -> NaiveDateTime {
    let min = profile.bolus().insulin_action_hours() * MIN_PER_HOUR;
    at + TimeDelta::minutes(min.round() as i64)
}

/// Shared exclusions: an occlusion, a settings change, a CGM gap. Returns
/// the readings from the last one at or before `from` to the first at or
/// after `to`.
fn readings<'a>(
    export: &'a Export,
    history: &ProfileHistory,
    entry: NaiveDateTime,
    from: NaiveDateTime,
    to: NaiveDateTime,
) -> Result<&'a [GlucoseReading], Exclusion> {
    let lookback = from - TimeDelta::minutes(OCCLUSION_LOOKBACK_MIN);
    if let Some(a) = export
        .alarms
        .iter()
        .find(|a| a.kind == OCCLUSION && a.at >= lookback && a.at <= to)
    {
        return Err(Exclusion::Occlusion(a.at));
    }
    if let Some((changed, _)) = history.at(to).filter(|(f, _)| *f != entry) {
        return Err(Exclusion::SettingsChange(changed));
    }
    gap_free(&export.cgm, from, to).ok_or(Exclusion::CgmGap)
}

fn gap_free(
    cgm: &[GlucoseReading],
    from: NaiveDateTime,
    to: NaiveDateTime,
) -> Option<&[GlucoseReading]> {
    let gap = TimeDelta::minutes(MAX_GAP_MIN);
    let lo = cgm.partition_point(|r| r.at <= from).checked_sub(1)?;
    let hi = cgm.partition_point(|r| r.at < to);
    let w = cgm.get(lo..=hi)?;
    // w starts at or before `from` and ends at or after `to`, so a late first
    // or early last reading shows up as a gap too.
    w.windows(PAIR)
        .all(|p| p[1].at - p[0].at <= gap)
        .then_some(w)
}

/// The last reading at or before `t`.
fn value_at(w: &[GlucoseReading], t: NaiveDateTime) -> MgDl {
    let i = w.partition_point(|r| r.at <= t).saturating_sub(1);
    w[i].mg()
}

fn meal_outcome(
    export: &Export,
    history: &ProfileHistory,
    (entry, profile): (NaiveDateTime, &Profile),
    at: NaiveDateTime,
    previous_meal: Option<NaiveDateTime>,
    next_meal: Option<NaiveDateTime>,
) -> Result<Outcome, Exclusion> {
    let end = action_end(profile, at);
    if let Some(prev) = previous_meal.filter(|&p| action_end(profile, p) > at) {
        return Err(Exclusion::PreviousMeal(prev));
    }
    if let Some(next) = next_meal.filter(|&n| n < end) {
        return Err(Exclusion::NextMeal(next));
    }
    let w = readings(export, history, entry, at, end)?;
    let inside: Vec<&GlucoseReading> = w.iter().filter(|r| r.at >= at && r.at <= end).collect();
    let mg: Vec<MgDl> = inside.iter().map(|r| r.mg()).collect();
    let peak = inside
        .iter()
        .copied()
        .max_by(|a, b| a.mg_dl.total_cmp(&b.mg_dl))
        .unwrap_or(&w[0]);
    let first_high = mg.iter().position(|&g| above_range(g));
    let spike_then_low = first_high.is_some_and(|i| mg[i..].iter().any(|&g| below_range(g)));
    let at_action_end = value_at(w, end);

    let late_end = at + TimeDelta::minutes(LATE_END_MIN);
    let late_rise = (next_meal.is_none_or(|n| n >= late_end))
        .then(|| gap_free(&export.cgm, end, late_end))
        .flatten()
        .map(|late| in_range(at_action_end) && late.iter().any(|r| above_range(r.mg())));

    // A correction in the meal's minute is part of its insulin.
    let sum = |kind, field: fn(&Bolus) -> f64| -> f64 {
        export
            .boluses
            .iter()
            .filter(|b| b.at >= at && b.at <= end && b.kind() == kind)
            .map(field)
            .sum()
    };
    let corrections = Units(sum(BolusKind::Correction, |b| b.units));
    let hypo_carbs = Grams(sum(BolusKind::HypoTreatment, |b| b.carbs_g));
    let rest_u_per_h = walsh_basal_u_per_h(profile.auto().total_daily_dose_u());
    let auto_extra = Units(
        export
            .basal
            .iter()
            .map(|s| {
                let s_end = s.at + TimeDelta::minutes(i64::from(s.minutes));
                let overlap = (s_end.min(end) - s.at.max(at)).num_minutes().max(0);
                (s.rate_u_h - rest_u_per_h) * overlap as f64 / MIN_PER_HOUR
            })
            .sum(),
    );
    Ok(Outcome {
        start: value_at(w, at),
        peak: MgDl(peak.mg_dl),
        peak_after: peak.at - at,
        lowest: MgDl(mg.iter().map(|g| g.0).fold(f64::INFINITY, f64::min)),
        at_action_end,
        corrections,
        hypo_carbs,
        auto_extra,
        spike_then_low,
        late_rise,
    })
}

fn correction_outcome(
    export: &Export,
    history: &ProfileHistory,
    (entry, profile): (NaiveDateTime, &Profile),
    this: &Bolus,
) -> Result<CorrectionOutcome, Exclusion> {
    let at = this.at;
    let end = action_end(profile, at);
    let before = at - (end - at);
    for b in export
        .boluses
        .iter()
        .filter(|b| b.at >= before && b.at <= end && !std::ptr::eq(*b, this))
    {
        match b.kind() {
            BolusKind::Correction if b.at >= at => return Err(Exclusion::Stacked(b.at)),
            BolusKind::Correction => {}
            _ => return Err(Exclusion::Carbs(b.at)),
        }
    }
    let w = readings(export, history, entry, at, end)?;
    Ok(CorrectionOutcome {
        start: value_at(w, at),
        at_action_end: value_at(w, end),
    })
}

/// Meals of one carb ratio block.
#[derive(Clone, Debug, PartialEq)]
pub struct MealSummary {
    /// The latest meal's block.
    pub block: Block,
    pub meals: usize,
    /// Meals without an exclusion; the counts after this are of judged meals.
    pub judged: usize,
    pub as_calculated: usize,
    pub in_range_at_end: usize,
    pub with_low: usize,
    pub corrected: usize,
    pub spike_then_low: usize,
    pub late_rise: usize,
    pub effective_carb_ratio: Option<GramsPerUnit>,
}

/// Corrections of one correction factor block.
#[derive(Clone, Debug, PartialEq)]
pub struct CorrectionSummary {
    /// The latest correction's block.
    pub block: Block,
    pub corrections: usize,
    pub judged: usize,
    pub drop_per_unit: Option<MgDlPerUnit>,
}

/// Per carb ratio block, in block order.
pub fn meals_by_block(meals: &[&Meal]) -> Vec<MealSummary> {
    group(meals, |m| m.block)
        .into_iter()
        .map(|(block, ms)| {
            let judged: Vec<&Outcome> = ms.iter().filter_map(|m| m.outcome.as_ref().ok()).collect();
            let count = |f: fn(&Outcome) -> bool| judged.iter().filter(|o| f(o)).count();
            MealSummary {
                block,
                meals: ms.len(),
                judged: judged.len(),
                as_calculated: ms.iter().filter(|m| m.dose == Dose::AsCalculated).count(),
                in_range_at_end: count(Outcome::in_range_at_end),
                with_low: count(|o| below_range(o.lowest)),
                corrected: count(|o| o.corrections > Units(0.0)),
                spike_then_low: count(|o| o.spike_then_low),
                late_rise: count(|o| o.late_rise == Some(true)),
                effective_carb_ratio: median(
                    ms.iter()
                        .filter_map(|m| m.effective_carb_ratio())
                        .map(|r| r.0),
                )
                .map(GramsPerUnit),
            }
        })
        .collect()
}

/// Per correction factor block, in block order.
pub fn corrections_by_block(corrections: &[&Correction]) -> Vec<CorrectionSummary> {
    group(corrections, |c| c.block)
        .into_iter()
        .map(|(block, cs)| CorrectionSummary {
            block,
            corrections: cs.len(),
            judged: cs.iter().filter(|c| c.outcome.is_ok()).count(),
            drop_per_unit: median(cs.iter().filter_map(|c| c.drop_per_unit()).map(|d| d.0))
                .map(MgDlPerUnit),
        })
        .collect()
}

/// Items by block span, in time-of-day order; each group keeps its latest
/// item's block, so the value is the one in force last.
fn group<'a, T>(items: &[&'a T], block: impl Fn(&T) -> Block) -> Vec<(Block, Vec<&'a T>)> {
    let mut groups: Vec<(Block, Vec<&'a T>)> = Vec::new();
    for &item in items {
        let b = block(item);
        let span = |g: &Block| (g.start_min, g.end_min) == (b.start_min, b.end_min);
        match groups.iter_mut().find(|g| span(&g.0)) {
            Some(g) => {
                g.0 = b;
                g.1.push(item);
            }
            None => groups.push((b, vec![item])),
        }
    }
    groups.sort_by_key(|g| g.0.start_min);
    groups
}

/// Upper median.
pub fn median(v: impl Iterator<Item = f64>) -> Option<f64> {
    let mut v: Vec<f64> = v.collect();
    v.sort_by(f64::total_cmp);
    v.get(v.len() / HALF).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use tir_tuner_common::data::{Alarm, BasalSegment};
    use tir_tuner_common::settings::ProfileInput;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, d).unwrap()
    }
    fn t(d: u32, h: u32, m: u32) -> NaiveDateTime {
        day(d).and_hms_opt(h, m, 0).unwrap()
    }

    /// CR 10 at breakfast (06-11) unless given, 12 otherwise; CF 100 by day,
    /// 80 at night; 2 h action; TDD 50, so the resting basal is 1 U/h.
    fn profile(breakfast_cr: f64) -> Profile {
        let b = |s: u16, e: u16, value| Block {
            start_min: s * 60,
            end_min: e * 60,
            value,
        };
        Profile::new(ProfileInput {
            carb_ratio: vec![b(6, 11, breakfast_cr), b(11, 6, 12.0)],
            correction_factor: vec![b(6, 22, 100.0), b(22, 6, 80.0)],
            bolus_target: vec![Block::all_day(110.0)],
            auto_target: None,
            insulin_action_hours: 2.0,
            max_bolus_u: 30.0,
            min_glucose_mg_dl: 70.0,
            weight_kg: 50.0,
            total_daily_dose_u: 50.0,
        })
        .unwrap()
    }

    fn history(entries: &[(NaiveDateTime, f64)]) -> ProfileHistory {
        ProfileHistory::new(entries.iter().map(|&(f, cr)| (f, profile(cr))).collect()).unwrap()
    }

    fn one_profile() -> ProfileHistory {
        history(&[(t(1, 0, 0), 10.0)])
    }

    /// A reading every 5 min from `from`, linear between (minute, mg/dL) points.
    fn cgm(from: NaiveDateTime, points: &[(i64, f64)]) -> Vec<GlucoseReading> {
        let last = points[points.len() - 1].0;
        (0..=last)
            .step_by(5)
            .map(|m| {
                let k = points.iter().rposition(|p| p.0 <= m).unwrap();
                let (m0, g0) = points[k];
                let (m1, g1) = *points.get(k + 1).unwrap_or(&points[k]);
                let g = if m1 == m0 {
                    g0
                } else {
                    g0 + (g1 - g0) * (m - m0) as f64 / (m1 - m0) as f64
                };
                GlucoseReading {
                    at: from + TimeDelta::minutes(m),
                    mg_dl: g,
                }
            })
            .collect()
    }

    fn bolus(at: NaiveDateTime, carbs_g: f64, units: f64) -> Bolus {
        Bolus { at, carbs_g, units }
    }

    fn export(cgm: Vec<GlucoseReading>, boluses: Vec<Bolus>) -> Export {
        Export {
            start: day(1),
            end: day(30),
            cgm,
            bg: vec![],
            boluses,
            basal: vec![],
            alarms: vec![],
        }
    }

    fn flat(from: NaiveDateTime, minutes: i64) -> Vec<GlucoseReading> {
        cgm(from, &[(0, 120.0), (minutes, 120.0)])
    }

    #[test]
    fn meal_is_judged_when_the_insulin_action_is_over() {
        // 120 at the 08:00 bolus, 200 an hour later, 150 at 10:00, 200 at 12:00.
        let mut e = export(
            cgm(
                t(10, 7, 0),
                &[
                    (0, 120.0),
                    (60, 120.0),
                    (120, 200.0),
                    (180, 150.0),
                    (300, 200.0),
                    (420, 120.0),
                ],
            ),
            vec![
                bolus(t(10, 8, 0), 50.0, 5.0),
                bolus(t(10, 8, 0), 0.0, 1.0),
                bolus(t(10, 9, 30), 15.0, 0.0),
                bolus(t(10, 10, 30), 0.0, 1.0),
                // After the late window: the late rise still counts.
                bolus(t(10, 14, 0), 30.0, 2.5),
            ],
        );
        // 1.5 U/h for the 2 h after the meal, 0.5 U above the resting 1 U/h.
        e.basal = vec![
            BasalSegment {
                at: t(10, 7, 0),
                minutes: 60,
                rate_u_h: 3.0,
            },
            BasalSegment {
                at: t(10, 8, 0),
                minutes: 120,
                rate_u_h: 1.5,
            },
            BasalSegment {
                at: t(10, 10, 0),
                minutes: 60,
                rate_u_h: 3.0,
            },
        ];
        let a = analyse(&e, &one_profile());
        let m = &a.meals[0];
        assert_eq!(
            (m.dose, m.calculated, m.block.label()),
            (Dose::AsCalculated, Units(5.0), "06-11".into())
        );
        let o = m.outcome.unwrap();
        assert_eq!(o.start, MgDl(120.0));
        assert_eq!(
            (o.peak, o.peak_after),
            (MgDl(200.0), TimeDelta::minutes(60))
        );
        assert_eq!((o.lowest, o.at_action_end), (MgDl(120.0), MgDl(150.0)));
        assert_eq!(
            (o.corrections, o.hypo_carbs, o.auto_extra),
            (Units(1.0), Grams(15.0), Units(1.0))
        );
        assert!(o.in_range_at_end() && !o.spike_then_low);
        assert_eq!(o.late_rise, Some(true));
        assert_eq!(m.effective_carb_ratio(), Some(GramsPerUnit(50.0 / 7.0)));
    }

    #[test]
    fn boluses_up_to_30_min_apart_are_one_meal() {
        let e = export(
            flat(t(10, 11, 0), 720),
            vec![
                bolus(t(10, 12, 0), 30.0, 2.5),
                bolus(t(10, 12, 30), 20.0, 1.7),
                bolus(t(10, 13, 0), 10.0, 0.8),
                bolus(t(10, 13, 31), 24.0, 2.0),
                bolus(t(10, 18, 0), 36.0, 3.0),
                // Right at the end of the 18:00 meal's action time.
                bolus(t(10, 20, 0), 36.0, 3.0),
            ],
        );
        let a = analyse(&e, &one_profile());
        let m = &a.meals;
        assert_eq!(
            (m.len(), m[0].boluses, m[0].carbs, m[0].dose),
            (4, 3, Grams(60.0), Dose::AsCalculated)
        );
        assert_eq!(m[0].outcome, Err(Exclusion::NextMeal(t(10, 13, 31))));
        assert_eq!(m[1].outcome, Err(Exclusion::PreviousMeal(t(10, 13, 0))));
        assert!(m[2].outcome.is_ok() && m[3].outcome.is_ok());
    }

    #[test]
    fn readings_outside_the_window_dont_count() {
        // 60 until 07:58, 120 from 08:03; the bolus is at 08:00.
        let mut c = cgm(
            t(10, 7, 3),
            &[(0, 60.0), (55, 60.0), (60, 120.0), (300, 120.0)],
        );
        c.retain(|r| r.mg_dl == 60.0 || r.mg_dl == 120.0);
        let a = analyse(
            &export(c, vec![bolus(t(10, 8, 0), 40.0, 4.0)]),
            &one_profile(),
        );
        let o = a.meals[0].outcome.unwrap();
        assert_eq!((o.start, o.lowest), (MgDl(60.0), MgDl(120.0)));
    }

    #[test]
    fn the_split_uses_the_settings_in_force() {
        let h = history(&[(t(10, 0, 0), 10.0), (t(11, 0, 0), 8.0), (t(12, 9, 0), 8.0)]);
        let e = export(
            [
                flat(t(10, 7, 0), 240),
                flat(t(11, 7, 0), 240),
                flat(t(12, 7, 0), 240),
            ]
            .concat(),
            vec![
                bolus(t(9, 8, 0), 50.0, 5.0),
                bolus(t(9, 9, 0), 0.0, 1.0),
                bolus(t(10, 8, 0), 50.0, 5.0),
                bolus(t(11, 8, 0), 50.0, 5.0),
                bolus(t(12, 8, 0), 48.0, 6.5),
            ],
        );
        let a = analyse(&e, &h);
        assert_eq!(a.without_settings, 2);
        let doses: Vec<Dose> = a.meals.iter().map(|m| m.dose).collect();
        assert_eq!(
            doses,
            [
                Dose::AsCalculated,
                Dose::LessThanCalculated,
                Dose::WithCorrection
            ]
        );
        assert_eq!(a.meals[1].block.value, 8.0);
        // The summary shows the ratio in force last.
        let s = meals_by_block(&a.meals.iter().collect::<Vec<_>>());
        assert_eq!(s[0].block.value, 8.0);
        assert!(a.meals[1].outcome.is_ok());
        assert_eq!(
            a.meals[2].outcome,
            Err(Exclusion::SettingsChange(t(12, 9, 0)))
        );
    }

    #[test]
    fn occlusions_and_cgm_gaps_exclude() {
        let gap = |from: i64, to: i64| {
            let mut c = flat(t(10, 0, 0), 1200);
            c.retain(|r| {
                r.at <= t(10, 16, 0) + TimeDelta::minutes(from)
                    || r.at >= t(10, 16, 0) + TimeDelta::minutes(to)
            });
            c
        };
        let alarm = |at, kind: &str| Alarm {
            at,
            kind: kind.into(),
        };
        let mut e = export(
            gap(30, 55),
            vec![
                bolus(t(10, 10, 0), 48.0, 4.0),
                bolus(t(10, 16, 0), 48.0, 4.0),
            ],
        );
        e.alarms = vec![
            alarm(t(10, 5, 0), OCCLUSION),
            alarm(t(10, 6, 0), "Low Battery"),
        ];
        let out = |e: &Export| -> Vec<Result<(), Exclusion>> {
            analyse(e, &one_profile())
                .meals
                .iter()
                .map(|m| m.outcome.map(|_| ()))
                .collect()
        };
        assert_eq!(out(&e), [Ok(()), Err(Exclusion::CgmGap)]);

        // 4 h before and at the action end still count, a 20 min gap doesn't.
        e.cgm = gap(30, 50);
        e.alarms = vec![
            alarm(t(10, 6, 0), OCCLUSION),
            alarm(t(10, 18, 0), OCCLUSION),
        ];
        assert_eq!(
            out(&e),
            [
                Err(Exclusion::Occlusion(t(10, 6, 0))),
                Err(Exclusion::Occlusion(t(10, 18, 0)))
            ]
        );
        e.alarms.clear();
        assert_eq!(out(&e), [Ok(()), Ok(())]);
    }

    #[test]
    fn spike_then_low_needs_the_high_first() {
        let e = export(
            [
                cgm(
                    t(10, 8, 0),
                    &[(0, 120.0), (40, 200.0), (80, 60.0), (120, 100.0)],
                ),
                cgm(
                    t(11, 8, 0),
                    &[
                        (0, 120.0),
                        (40, 60.0),
                        (80, 200.0),
                        (120, 200.0),
                        (180, 250.0),
                        (360, 250.0),
                    ],
                ),
            ]
            .concat(),
            vec![bolus(t(10, 8, 0), 40.0, 4.0), bolus(t(11, 8, 0), 40.0, 4.0)],
        );
        let a = analyse(&e, &one_profile());
        let o: Vec<Outcome> = a.meals.iter().map(|m| m.outcome.unwrap()).collect();
        assert!(o[0].spike_then_low && !o[1].spike_then_low);
        assert_eq!(o[0].lowest, MgDl(60.0));
        // Out of range at the end: a higher reading later isn't a late rise.
        // The first day's late window has no readings.
        assert_eq!((o[0].late_rise, o[1].late_rise), (None, Some(false)));
    }

    #[test]
    fn corrections_across_midnight_and_what_spoils_them() {
        let e = export(
            [cgm(
                t(10, 22, 0),
                &[(0, 250.0), (60, 250.0), (180, 90.0), (600, 90.0)],
            )]
            .concat(),
            vec![
                bolus(t(10, 23, 0), 0.0, 2.0),
                bolus(t(11, 2, 0), 15.0, 0.0),
                bolus(t(11, 3, 0), 0.0, 1.0),
                bolus(t(11, 5, 0), 0.0, 1.0),
                bolus(t(11, 5, 30), 0.0, 1.0),
                bolus(t(11, 23, 0), 0.0, 1.0),
                bolus(t(11, 23, 0), 0.0, 1.0),
            ],
        );
        let a = analyse(&e, &one_profile());
        let out: Vec<_> = a
            .corrections
            .iter()
            .map(|c| c.outcome.map(|_| ()))
            .collect();
        assert_eq!(
            out,
            [
                Ok(()),
                Err(Exclusion::Carbs(t(11, 2, 0))),
                Err(Exclusion::Stacked(t(11, 5, 30))),
                Ok(()),
                // Two in the same minute stack.
                Err(Exclusion::Stacked(t(11, 23, 0))),
                Err(Exclusion::Stacked(t(11, 23, 0))),
            ]
        );
        assert_eq!(a.corrections[0].drop_per_unit(), Some(MgDlPerUnit(80.0)));
        let s = corrections_by_block(&a.corrections.iter().collect::<Vec<_>>());
        assert_eq!(s.len(), 1);
        assert_eq!(
            (s[0].block.label(), s[0].corrections, s[0].judged),
            ("22-06".into(), 6, 2)
        );
        assert_eq!(
            (s[0].block.value, s[0].drop_per_unit),
            (80.0, Some(MgDlPerUnit(80.0)))
        );
    }

    #[test]
    fn block_summary_counts_judged_meals() {
        let e = export(
            [
                cgm(
                    t(10, 7, 0),
                    &[(0, 120.0), (120, 250.0), (180, 60.0), (240, 120.0)],
                ),
                cgm(t(10, 12, 0), &[(0, 120.0), (240, 120.0)]),
                cgm(t(11, 7, 0), &[(0, 120.0), (180, 150.0), (420, 150.0)]),
            ]
            .concat(),
            vec![
                bolus(t(10, 8, 0), 50.0, 5.0),
                bolus(t(10, 9, 30), 0.0, 1.0),
                bolus(t(10, 13, 0), 48.0, 4.0),
                bolus(t(11, 8, 0), 50.0, 6.0),
                bolus(t(11, 23, 0), 24.0, 2.0),
            ],
        );
        let a = analyse(&e, &one_profile());
        let s = meals_by_block(&a.meals.iter().collect::<Vec<_>>());
        let blocks: Vec<String> = s.iter().map(|s| s.block.label()).collect();
        assert_eq!(blocks, ["06-11", "11-06"]);
        let b = &s[0];
        assert_eq!(
            (b.block.value, b.meals, b.judged, b.as_calculated),
            (10.0, 2, 2, 1)
        );
        assert_eq!(
            (
                b.in_range_at_end,
                b.with_low,
                b.corrected,
                b.spike_then_low,
                b.late_rise
            ),
            (1, 1, 1, 1, 0)
        );
        // 50 g over 6 U and over 5 + 1 U: both 8.33.
        assert_eq!(b.effective_carb_ratio, Some(GramsPerUnit(50.0 / 6.0)));
        assert_eq!(
            (s[1].meals, s[1].judged, s[1].as_calculated, s[1].corrected),
            (2, 1, 2, 0)
        );
    }

    #[test]
    fn upper_median() {
        assert_eq!(median([3.0, 1.0, 2.0].into_iter()), Some(2.0));
        assert_eq!(median([4.0, 1.0, 3.0, 2.0].into_iter()), Some(3.0));
        assert_eq!(median(std::iter::empty()), None);
    }
}
