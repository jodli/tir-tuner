//! Clinical reporting metrics over glucose time series, all in mmol/L.
//!
//! These are the alignment-free outcome measures used by the trial
//! literature (Patek 2009 `[P09]`, Wilinska 2010 `[W10]`): percent time in range with
//! the ISO-defined 3.9-10.0 mmol/L band, mean glucose, and coefficient
//! of variation. Metrics are pure functions over slices so the numeric
//! integration and the reporting stay independent.

use crate::units::mmol_per_l_to_mg_per_dl;
use document_formulas::formula_doc;

/// Target range lower bound, mmol/L (70 mg/dL).
pub const TIME_IN_RANGE_MIN_MMOL_L: f64 = 3.9;
/// Target range upper bound, mmol/L (180 mg/dL).
pub const TIME_IN_RANGE_MAX_MMOL_L: f64 = 10.0;

/// Fraction of glucose samples inside the [3.9, 10.0] mmol/L band,
/// expressed as a percentage. Samples that are NaN are ignored so a
/// dropped CGM reading does not silently depress the result.
#[formula_doc]
pub fn time_in_range_pct(samples_mmol_per_l: &[f64]) -> f64 {
    if samples_mmol_per_l.is_empty() {
        return f64::NAN;
    }
    let mut in_range = 0usize;
    let mut counted = 0usize;
    for &v in samples_mmol_per_l {
        if v.is_nan() {
            continue;
        }
        counted += 1;
        if (TIME_IN_RANGE_MIN_MMOL_L..=TIME_IN_RANGE_MAX_MMOL_L).contains(&v) {
            in_range += 1;
        }
    }
    if counted == 0 {
        f64::NAN
    } else {
        tir_percent(in_range, counted)
    }
}

/// Percent time in range from the in-range and counted sample totals.
/// Split out of [`time_in_range_pct`] so the crate can render the ratio:
/// the extractor does not descend into the counting loop.
#[formula_doc]
#[allow(clippy::let_and_return)] // keep the assigned name as the formula symbol
pub fn tir_percent(in_range: usize, counted: usize) -> f64 {
    let pct = 100.0 * in_range as f64 / counted as f64;
    pct
}

/// Mean glucose over non-NaN samples, mmol/L.
#[formula_doc]
pub fn mean_glucose(samples_mmol_per_l: &[f64]) -> f64 {
    let mut sum = 0.0;
    let mut counted = 0usize;
    for &v in samples_mmol_per_l {
        if v.is_nan() {
            continue;
        }
        sum += v;
        counted += 1;
    }
    if counted == 0 {
        f64::NAN
    } else {
        mean_of(sum, counted)
    }
}

/// Arithmetic mean of the sample sum and count. Split out of
/// [`mean_glucose`] so the crate can render the ratio.
#[formula_doc]
#[allow(clippy::let_and_return)] // keep the assigned name as the formula symbol
pub fn mean_of(sum: f64, counted: usize) -> f64 {
    let mean = sum / counted as f64;
    mean
}

/// Coefficient of variation of glucose, percent. Requires at least two
/// non-NaN samples; otherwise NaN.
#[formula_doc]
pub fn coefficient_of_variation(samples_mmol_per_l: &[f64]) -> f64 {
    let mean = mean_glucose(samples_mmol_per_l);
    if mean.is_nan() {
        return f64::NAN;
    }
    let mut count = 0usize;
    let mut sq_err = 0.0;
    for &v in samples_mmol_per_l {
        if v.is_nan() {
            continue;
        }
        let e = v - mean;
        sq_err += e * e;
        count += 1;
    }
    if count < 2 || mean == 0.0 {
        f64::NAN
    } else {
        cv_pct(sq_err, count, mean)
    }
}

/// Coefficient of variation from the summed squared deviation about
/// the mean: the sample standard deviation over the mean, scaled to
/// percent. Split out of
/// [`coefficient_of_variation`] so the crate can render it.
#[formula_doc]
#[allow(clippy::let_and_return)] // keep the assigned name as the formula symbol
pub fn cv_pct(sq_err: f64, count: usize, mean: f64) -> f64 {
    let sd = (sq_err / (count as f64 - 1.0)).sqrt();
    let cv = 100.0 * sd / mean;
    cv
}

/// Low blood glucose index, the Kovatchev risk score. Readings above
/// ~112.5 mg/dL (where the risk transform changes sign) contribute
/// nothing. Requires at least one non-NaN sample.
pub fn low_bgi(samples_mmol_per_l: &[f64]) -> f64 {
    risk_score(samples_mmol_per_l, RiskSide::Low)
}

/// High blood glucose index, the Kovatchev risk score. Readings below
/// ~112.5 mg/dL contribute nothing. Requires at least one non-NaN sample.
pub fn high_bgi(samples_mmol_per_l: &[f64]) -> f64 {
    risk_score(samples_mmol_per_l, RiskSide::High)
}

#[derive(Clone, Copy)]
enum RiskSide {
    Low,
    High,
}

fn risk_score(samples_mmol_per_l: &[f64], side: RiskSide) -> f64 {
    let mut total = 0.0;
    let mut counted = 0usize;
    for &v in samples_mmol_per_l {
        if v.is_nan() {
            continue;
        }
        total += risk_if_on_side(v, matches!(side, RiskSide::Low));
        counted += 1;
    }
    if counted == 0 {
        f64::NAN
    } else {
        mean_of(total, counted)
    }
}

/// The Kovatchev risk contribution of one glucose reading `v` (mmol/L)
/// on a given side of the risk transform.
///
/// The Kovatchev transform maps a reading to a risk scale where the
/// sign flips at ~112.5 mg/dL; the score is the squared distance of the
/// deviation (Kovatchev 2017, section 6.2). Readings on the wrong side
/// of the flip contribute zero. Split out of the averaging loop so the
/// crate can render the transform.
#[formula_doc]
#[allow(clippy::let_and_return)] // keep the assigned name as the formula symbol
pub fn risk_if_on_side(value_mmol_per_l: f64, low_side: bool) -> f64 {
    let mg_dl = mmol_per_l_to_mg_per_dl(value_mmol_per_l);
    let f = 1.509 * (mg_dl.ln().powf(1.084) - 5.381);
    let score = 10.0 * f * f;
    let contributes = if low_side { f < 0.0 } else { f >= 0.0 };
    if contributes {
        score
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tir_counts_band_boundaries_inclusive() {
        let s = [3.9, 10.0, 5.8, 2.5, 13.9];
        // 3.9 and 10.0 count as in-range, 5.8 counts, the lows/highs don't.
        assert!((time_in_range_pct(&s) - 60.0).abs() < 1e-9);
    }

    #[test]
    fn tir_ignores_nan_readings() {
        assert!((time_in_range_pct(&[5.8, f64::NAN, 6.5]) - 100.0).abs() < 1e-9);
        assert!(time_in_range_pct(&[f64::NAN]).is_nan());
        assert!(time_in_range_pct(&[]).is_nan());
    }

    #[test]
    fn mean_and_cv_on_known_series() {
        let s = [5.0, 5.0, 5.0];
        assert!((mean_glucose(&s) - 5.0).abs() < 1e-12);
        assert!((coefficient_of_variation(&s) - 0.0).abs() < 1e-12);
        assert!(coefficient_of_variation(&[5.0]).is_nan());
    }

    #[test]
    fn risk_scores_separate_by_sign() {
        // 5.8 mmol/L = 104.5 mg/dL sits below the sign-crossing point,
        // so it is entirely low-risk; 13.9 mmol/L = 250 mg/dL is high.
        assert!(low_bgi(&[5.8]) > 0.0);
        assert!(low_bgi(&[5.8]) < 1.0);
        assert!(high_bgi(&[5.8]) == 0.0);
        assert!(high_bgi(&[13.9]) > 1.0);
        assert!(low_bgi(&[13.9]) == 0.0);
    }

    proptest::proptest! {
        #[test]
        fn tir_is_bounded_percentage(values in proptest::collection::vec(-20.0f64..50.0, 0..100)) {
            let pct = time_in_range_pct(&values);
            if values.is_empty() {
                assert!(pct.is_nan());
            } else {
                assert!((0.0..=100.0).contains(&pct));
            }
        }
    }
}
