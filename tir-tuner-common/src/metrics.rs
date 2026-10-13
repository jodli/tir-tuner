//! CGM metrics as the international consensus on time in range defines them
//! (Battelino 2019, Table 2). Readings are mg/dL; NaN readings are skipped.

use crate::units::MgDl;

/// Lowest reading in range, mg/dL. src: battelino-tir-band
pub const TIR_LOW_MG_DL: f64 = 70.0;
/// Highest reading in range, mg/dL. src: battelino-tir-band
pub const TIR_HIGH_MG_DL: f64 = 180.0;

/// Below this is level 2 hypoglycemia, mg/dL. src: battelino-level2-bands
pub const TBR_VERY_LOW_MG_DL: f64 = 54.0;
/// Above this is level 2 hyperglycemia, mg/dL. src: battelino-level2-bands
pub const TAR_VERY_HIGH_MG_DL: f64 = 250.0;

/// src: percent
const PERCENT: f64 = 100.0;

/// A reading in 70-180 mg/dL, on Battelino's whole mg/dL.
pub fn in_range(g: MgDl) -> bool {
    (TIR_LOW_MG_DL..=TIR_HIGH_MG_DL).contains(&g.0.round())
}

/// A reading below 70 mg/dL, on Battelino's whole mg/dL.
pub fn below_range(g: MgDl) -> bool {
    g.0.round() < TIR_LOW_MG_DL
}

/// A reading above 180 mg/dL, on Battelino's whole mg/dL.
pub fn above_range(g: MgDl) -> bool {
    g.0.round() > TIR_HIGH_MG_DL
}

/// Percent of readings in 70-180 mg/dL.
pub fn time_in_range_pct(readings: &[MgDl]) -> f64 {
    pct(readings, |mg| in_range(MgDl(mg)))
}

/// Percent of readings below 70 mg/dL.
pub fn time_below_range_pct(readings: &[MgDl]) -> f64 {
    pct(readings, |mg| below_range(MgDl(mg)))
}

/// Percent of readings very low, low, in range, high and very high.
pub fn range_bands_pct(readings: &[MgDl]) -> [f64; 5] {
    [
        pct(readings, |mg| mg < TBR_VERY_LOW_MG_DL),
        pct(readings, |mg| {
            (TBR_VERY_LOW_MG_DL..TIR_LOW_MG_DL).contains(&mg)
        }),
        time_in_range_pct(readings),
        pct(readings, |mg| {
            mg > TIR_HIGH_MG_DL && mg <= TAR_VERY_HIGH_MG_DL
        }),
        pct(readings, |mg| mg > TAR_VERY_HIGH_MG_DL),
    ]
}

fn valid(readings: &[MgDl]) -> impl Iterator<Item = f64> + Clone + '_ {
    readings.iter().map(|r| r.0).filter(|v| !v.is_nan())
}

/// Battelino's bands have integer edges ("54-69", "70-180", "181-250"), as
/// CGM readings are whole mg/dL. A simulated reading is rounded to one first,
/// so 69.6 counts as 70. src: battelino-integer-bands
fn pct(readings: &[MgDl], hit: impl Fn(f64) -> bool) -> f64 {
    let (n, hits) = valid(readings).fold((0, 0), |(n, h), mg| {
        (n + 1, h + usize::from(hit(mg.round())))
    });
    if n == 0 {
        return f64::NAN;
    }
    PERCENT * hits as f64 / n as f64
}

/// Mean glucose. src: battelino-mean
pub fn mean_glucose(readings: &[MgDl]) -> MgDl {
    let (n, sum) = valid(readings).fold((0, 0.0), |(n, s), v| (n + 1, s + v));
    MgDl(sum / n as f64)
}

/// %CV: sample standard deviation over the mean. Battelino names the metric
/// but not the formula; n-1 vs n doesn't matter at CGM sample counts.
/// src: battelino-cv
pub fn coefficient_of_variation(readings: &[MgDl]) -> f64 {
    /// Sample SD needs two readings. src: battelino-cv
    const MIN_READINGS: usize = 2;
    let n = valid(readings).count();
    if n < MIN_READINGS {
        return f64::NAN;
    }
    let mean = mean_glucose(readings).0;
    let var = valid(readings)
        .map(|x| {
            let d = x - mean;
            d * d
        })
        .sum::<f64>()
        / (n - 1) as f64;
    PERCENT * var.sqrt() / mean
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mg(v: &[f64]) -> Vec<MgDl> {
        v.iter().map(|&x| MgDl(x)).collect()
    }

    #[test]
    fn battelino_tir_band_70_and_180_are_in_range() {
        let r = mg(&[54.0, 69.0, 70.0, 180.0, 181.0]);
        assert_eq!(time_in_range_pct(&r), 40.0);
        assert_eq!(time_below_range_pct(&r), 40.0);
    }

    #[test]
    fn battelino_level2_bands_split_54_and_250() {
        let r = mg(&[
            40.0, 53.0, 54.0, 69.0, 180.0, 181.0, 200.0, 250.0, 251.0, 300.0,
        ]);
        assert_eq!(range_bands_pct(&r), [20.0, 20.0, 10.0, 30.0, 20.0]);
    }

    #[test]
    fn battelino_integer_bands_round_simulated_readings() {
        let r = mg(&[69.4, 69.6, 180.4, 180.6]);
        assert_eq!(time_in_range_pct(&r), 50.0);
        assert_eq!(time_below_range_pct(&r), 25.0);
    }

    #[test]
    fn range_edges_are_whole_mg_dl() {
        let at = |g| {
            (
                below_range(MgDl(g)),
                in_range(MgDl(g)),
                above_range(MgDl(g)),
            )
        };
        assert_eq!(at(69.4), (true, false, false));
        assert_eq!(at(69.5), (false, true, false));
        assert_eq!(at(180.4), (false, true, false));
        assert_eq!(at(180.5), (false, false, true));
    }

    #[test]
    fn nan_readings_are_skipped() {
        let r = mg(&[100.0, f64::NAN, 300.0]);
        assert_eq!(time_in_range_pct(&r), 50.0);
        assert_eq!(mean_glucose(&r), MgDl(200.0));
        assert!(time_in_range_pct(&mg(&[f64::NAN])).is_nan());
    }

    #[test]
    fn battelino_cv_is_sample_sd_over_mean() {
        // mean 150, sample SD 50.
        assert!((coefficient_of_variation(&mg(&[100.0, 150.0, 200.0])) - 100.0 / 3.0).abs() < 1e-9);
        // Two readings: mean 150, sample SD 50 * sqrt(2).
        let two = coefficient_of_variation(&mg(&[100.0, 200.0]));
        assert!((two - 100.0 * 50.0 * 2f64.sqrt() / 150.0).abs() < 1e-9);
        assert!(coefficient_of_variation(&mg(&[100.0])).is_nan());
    }
}
