//! Units the medical path computes in. Each quantity is its own type, so a
//! conversion is a named call and mixing units doesn't compile.
#![allow(missing_docs)]

use std::ops::{Add, Div, Sub};

/// Glucose (C6H12O6) molar mass, g/mol, from the IUPAC standard atomic
/// weights C 12.011, H 1.008, O 15.999. src: glucose-molar-mass
pub const GLUCOSE_G_PER_MOL: f64 = 180.156;

/// mg/dL per mmol/L: one mmol/L is 180.156 mg/L, a litre is 10 dL.
/// src: glucose-molar-mass
pub const MG_PER_DL_PER_MMOL_PER_L: f64 = GLUCOSE_G_PER_MOL / 10.0;

/// mmol of glucose per gram of carbohydrate, a gram of carbs counted as a
/// gram of glucose. src: glucose-molar-mass
pub const MMOL_PER_GRAM_CHO: f64 = 1000.0 / GLUCOSE_G_PER_MOL;

/// Milli-units per insulin unit, the mass-state unit of the models.
/// src: si-milli
pub const MU_PER_UNIT: f64 = 1000.0;

/// Minutes per hour, for U/h rates in per-minute models. src: clock
pub const MIN_PER_HOUR: f64 = 60.0;

/// Glucose concentration, mg/dL: what the CGM, the export and the mg/dL app
/// show.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct MgDl(pub f64);

/// Glucose concentration, mmol/L: what the manual and the papers use.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct MmolL(pub f64);

/// Carbohydrate, g.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Grams(pub f64);

/// Insulin, U.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Units(pub f64);

/// Carb ratio: grams covered by one unit.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct GramsPerUnit(pub f64);

/// Correction factor: glucose drop per unit.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct MgDlPerUnit(pub f64);

impl MgDl {
    pub fn to_mmol_l(self) -> MmolL {
        MmolL(self.0 / MG_PER_DL_PER_MMOL_PER_L)
    }
}

impl MmolL {
    pub fn to_mg_dl(self) -> MgDl {
        MgDl(self.0 * MG_PER_DL_PER_MMOL_PER_L)
    }
}

/// A simulated trace (mmol/L) as readings the metrics take.
pub fn mg_dl_series(mmol_l: &[f64]) -> Vec<MgDl> {
    mmol_l.iter().map(|&v| MmolL(v).to_mg_dl()).collect()
}

impl Sub for MgDl {
    type Output = MgDl;
    fn sub(self, rhs: MgDl) -> MgDl {
        MgDl(self.0 - rhs.0)
    }
}

impl Units {
    pub fn max(self, other: Units) -> Units {
        Units(self.0.max(other.0))
    }

    pub fn clamp(self, lo: Units, hi: Units) -> Units {
        Units(self.0.clamp(lo.0, hi.0))
    }
}

impl Add for Units {
    type Output = Units;
    fn add(self, rhs: Units) -> Units {
        Units(self.0 + rhs.0)
    }
}

impl Sub for Units {
    type Output = Units;
    fn sub(self, rhs: Units) -> Units {
        Units(self.0 - rhs.0)
    }
}

/// Meal part: carbs over the carb ratio.
impl Div<GramsPerUnit> for Grams {
    type Output = Units;
    fn div(self, cr: GramsPerUnit) -> Units {
        Units(self.0 / cr.0)
    }
}

/// Correction part: glucose above target over the correction factor.
impl Div<MgDlPerUnit> for MgDl {
    type Output = Units;
    fn div(self, cf: MgDlPerUnit) -> Units {
        Units(self.0 / cf.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mg_dl_round_trips() {
        for v in [54.0, 70.0, 180.0, 250.0] {
            assert!((MgDl(v).to_mmol_l().to_mg_dl().0 - v).abs() < 1e-9);
        }
    }
}
