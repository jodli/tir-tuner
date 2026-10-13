//! The `report` command's text: overall metrics, then meals and corrections
//! per settings block, for the whole window and the recent days.

use std::fmt::Write;

use chrono::{NaiveDate, NaiveDateTime};
use tir_tuner_analysis::{Analysis, Dose, Exclusion, corrections_by_block, meals_by_block};
use tir_tuner_common::data::Export;
use tir_tuner_common::metrics::{
    TIR_HIGH_MG_DL, TIR_LOW_MG_DL, coefficient_of_variation, mean_glucose, time_below_range_pct,
    time_in_range_pct,
};
use tir_tuner_common::units::MgDl;

/// The report for `export`, recent = on or after `recent_from`.
pub fn render(export: &Export, a: &Analysis, recent_from: NaiveDate, verbose: bool) -> String {
    let mut out = String::new();
    let o = &mut out;
    let cgm: Vec<MgDl> = export.cgm.iter().map(|r| r.mg()).collect();
    let days = (export.end - export.start).num_days() + 1;
    let _ = writeln!(
        o,
        "{} .. {} ({days} days): TIR {:.1}%, TBR {:.1}%, mean {:.0} mg/dL, CV {:.1}%",
        export.start,
        export.end,
        time_in_range_pct(&cgm),
        time_below_range_pct(&cgm),
        mean_glucose(&cgm).0,
        coefficient_of_variation(&cgm),
    );
    if a.without_settings > 0 {
        let _ = writeln!(
            o,
            "{} boluses before the first settings entry, left out",
            a.without_settings
        );
    }
    for (title, from) in [
        (format!("all {days} days"), None),
        (format!("since {recent_from}"), Some(recent_from)),
    ] {
        let _ = writeln!(
            o,
            "\nMeals, {title}. Judged = no exclusion; the columns after it count judged meals."
        );
        let _ = writeln!(
            o,
            "{:<11} {:>4} {:>5} {:>6} {:>7} {:>8} {:>4} {:>9} {:>10} {:>9} {:>7}",
            "block",
            "CR",
            "meals",
            "as CR",
            "judged",
            "in range",
            "low",
            "corrected",
            "spike+low",
            "late rise",
            "eff. CR"
        );
        for s in meals_by_block(&since(&a.meals, from, |m| m.at)) {
            let _ = writeln!(
                o,
                "{:<11} {:>4} {:>5} {:>6} {:>7} {:>8} {:>4} {:>9} {:>10} {:>9} {:>7}",
                s.block.label(),
                s.block.value,
                s.meals,
                s.as_calculated,
                s.judged,
                s.in_range_at_end,
                s.with_low,
                s.corrected,
                s.spike_then_low,
                s.late_rise,
                s.effective_carb_ratio
                    .map_or("-".into(), |r| format!("{:.1}", r.0)),
            );
        }
        let _ = writeln!(
            o,
            "\nCorrections, {title}. Auto mode backs off after one, so the drop overstates the factor."
        );
        let _ = writeln!(
            o,
            "{:<11} {:>4} {:>11} {:>7} {:>9}",
            "block", "CF", "corrections", "judged", "drop/U"
        );
        for s in corrections_by_block(&since(&a.corrections, from, |c| c.at)) {
            let _ = writeln!(
                o,
                "{:<11} {:>4} {:>11} {:>7} {:>9}",
                s.block.label(),
                s.block.value,
                s.corrections,
                s.judged,
                s.drop_per_unit
                    .map_or("-".into(), |d| format!("{:.0}", d.0)),
            );
        }
    }
    let _ = writeln!(
        o,
        "\nin range = {TIR_LOW_MG_DL}-{TIR_HIGH_MG_DL} mg/dL when the insulin action time is over. eff. CR = carbs over \
         bolus, corrections and Auto mode insulin above the resting basal by then (median)."
    );
    if verbose {
        let _ = writeln!(o, "\nEvery meal:");
        for m in &a.meals {
            let dose = match m.dose {
                Dose::AsCalculated => "as CR",
                Dose::WithCorrection => "more",
                Dose::LessThanCalculated => "less",
            };
            let _ = write!(
                o,
                "{} {:>3} g {:>5.1} U ({dose}, CR {} gives {:.1}) ",
                m.at.format("%m-%d %H:%M"),
                m.carbs.0,
                m.units.0,
                m.block.value,
                m.calculated.0
            );
            let _ = match &m.outcome {
                Ok(r) => writeln!(
                    o,
                    "{:.0} -> peak {:.0} after {} min, low {:.0}, {:.0} at end{}{}{}{}",
                    r.start.0,
                    r.peak.0,
                    r.peak_after.num_minutes(),
                    r.lowest.0,
                    r.at_action_end.0,
                    if r.corrections.0 > 0.0 {
                        format!(", +{:.1} U corr", r.corrections.0)
                    } else {
                        String::new()
                    },
                    if r.hypo_carbs.0 > 0.0 {
                        format!(", {} g hypo", r.hypo_carbs.0)
                    } else {
                        String::new()
                    },
                    if r.spike_then_low { ", spike+low" } else { "" },
                    if r.late_rise == Some(true) {
                        ", late rise"
                    } else {
                        ""
                    },
                ),
                Err(e) => writeln!(o, "excluded: {}", exclusion(e)),
            };
        }
    }
    out
}

fn exclusion(e: &Exclusion) -> String {
    match e {
        Exclusion::CgmGap => "CGM gap".into(),
        Exclusion::NextMeal(t) => format!("next meal {}", t.format("%H:%M")),
        Exclusion::PreviousMeal(t) => format!("previous meal {}", t.format("%H:%M")),
        Exclusion::Carbs(t) => format!("carbs {}", t.format("%H:%M")),
        Exclusion::Stacked(t) => format!("another correction {}", t.format("%H:%M")),
        Exclusion::Occlusion(t) => format!("occlusion {}", t.format("%m-%d %H:%M")),
        Exclusion::SettingsChange(t) => format!("settings changed {t}"),
    }
}

fn since<T>(items: &[T], from: Option<NaiveDate>, at: impl Fn(&T) -> NaiveDateTime) -> Vec<&T> {
    items
        .iter()
        .filter(|i| from.is_none_or(|f| at(i).date() >= f))
        .collect()
}
