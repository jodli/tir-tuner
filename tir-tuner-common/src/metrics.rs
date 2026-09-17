//! Clinical reporting metrics over glucose time series, all in mmol/L.
//!
//! These are the alignment-free outcome measures used by the trial
//! literature (Patek 2009, Wilinska 2010): percent time in range with
//! the ISO-defined 3.9-10.0 mmol/L band, mean glucose, and coefficient
//! of variation. Metrics are pure functions over slices so the numeric
//! integration and the reporting stay independent.

use crate::units::mmol_per_l_to_mg_per_dl;

/// Target range lower bound, mmol/L (70 mg/dL).
pub const TIME_IN_RANGE_MIN_MMOL_L: f64 = 3.9;
/// Target range upper bound, mmol/L (180 mg/dL).
pub const TIME_IN_RANGE_MAX_MMOL_L: f64 = 10.0;

/// Fraction of glucose samples inside the [3.9, 10.0] mmol/L band,
/// expressed as a percentage. Samples that are NaN are ignored so a
/// dropped CGM reading does not silently depress the result.
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
        100.0 * in_range as f64 / counted as f64
    }
}

/// Mean glucose over non-NaN samples, mmol/L.
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
        sum / counted as f64
    }
}

/// Coefficient of variation of glucose, percent: `stddev / mean * 100`.
/// Requires at least two non-NaN samples; otherwise NaN.
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
        100.0 * (sq_err / (count as f64 - 1.0)).sqrt() / mean
    }
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
        let mg = mmol_per_l_to_mg_per_dl(v);
        let f = 1.509 * (mg.ln().powf(1.084) - 5.381);
        let contribute = match side {
            RiskSide::Low => f < 0.0,
            RiskSide::High => f >= 0.0,
        };
        if contribute {
            // G* in Kovatchev's notation, the "distance" component is
            // 10*f^2 with f in the 0..1.00001 scale.
            total += 10.0 * f * f;
        }
        counted += 1;
    }
    if counted == 0 {
        f64::NAN
    } else {
        total / counted as f64
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