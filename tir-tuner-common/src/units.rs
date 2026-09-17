//! Unit conversion constants and helpers shared across the workspace.

/// Millie-units per insulin unit, the `i1`/`i2` mass-state convention
/// (Hovorka et al. 2004).
pub const MU_PER_UNIT: f64 = 1000.0;

/// Milligrams per deciliter per millimole per liter. The US unit is
/// exactly `18.0182` times the SI unit for glucose
/// (180.156 g/mol / 10).
pub const MG_PER_DL_PER_MMOL_PER_L: f64 = 18.0182;

/// Millimoles of glucose per gram of carbohydrate.
///
/// Glucose molar mass 180.156 g/mol; a gram of carbohydrate is treated
/// as a gram of glucose, giving `1/0.180156` mmol.
pub const MMOL_PER_GRAM_CHO: f64 = 5.551;

/// Convert a glucose concentration from mmol/L to mg/dL.
pub fn mmol_per_l_to_mg_per_dl(value_mmol_per_l: f64) -> f64 {
    value_mmol_per_l * MG_PER_DL_PER_MMOL_PER_L
}

/// Convert a glucose concentration from mg/dL to mmol/L.
pub fn mg_per_dl_to_mmol_per_l(value_mg_per_dl: f64) -> f64 {
    value_mg_per_dl / MG_PER_DL_PER_MMOL_PER_L
}

/// Convert a carbohydrate mass from grams to mmol of glucose, the unit
/// the physiological models carry it in.
pub fn grams_cho_to_mmol(grams: f64) -> f64 {
    grams * MMOL_PER_GRAM_CHO
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mg_dl_conversion_round_trips() {
        for v in [3.9, 5.8, 10.0, 13.9] {
            let mg = mmol_per_l_to_mg_per_dl(v);
            let back = mg_per_dl_to_mmol_per_l(mg);
            assert!((back - v).abs() < 1e-9);
        }
    }

    #[test]
    fn known_glucose_conversion() {
        // 5.8 mmol/L is 104.5 mg/dL, the CamAPS target (Ware 2022).
        assert!((mmol_per_l_to_mg_per_dl(5.8) - 104.5).abs() < 0.05);
    }

    #[test]
    fn grams_cho_conversion() {
        assert!((grams_cho_to_mmol(50.0) - 277.55).abs() < 1e-9);
    }
}