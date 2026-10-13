//! Recorded therapy data: what the pump and CGM logged, in local time.
#![allow(missing_docs)]

use chrono::{NaiveDate, NaiveDateTime};

use crate::units::MgDl;

/// A CGM sample or fingerstick reading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlucoseReading {
    pub at: NaiveDateTime,
    pub mg_dl: f64,
}

impl GlucoseReading {
    pub fn mg(&self) -> MgDl {
        MgDl(self.mg_dl)
    }
}

/// A manual bolus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bolus {
    pub at: NaiveDateTime,
    pub carbs_g: f64,
    pub units: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BolusKind {
    Meal,
    /// Carbs logged with no insulin: the user treats lows this way.
    /// src: user-hypo-treatment
    HypoTreatment,
    /// No carbs.
    Correction,
}

impl Bolus {
    pub fn kind(&self) -> BolusKind {
        match (self.carbs_g > 0.0, self.units > 0.0) {
            (false, _) => BolusKind::Correction,
            (true, true) => BolusKind::Meal,
            (true, false) => BolusKind::HypoTreatment,
        }
    }
}

/// A stretch of constant basal rate set by the closed loop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BasalSegment {
    pub at: NaiveDateTime,
    pub minutes: u32,
    pub rate_u_h: f64,
}

/// One export. Series are sorted by time, `cgm` has at most one sample per minute.
#[derive(Clone, Debug, PartialEq)]
pub struct Export {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub cgm: Vec<GlucoseReading>,
    pub bg: Vec<GlucoseReading>,
    pub boluses: Vec<Bolus>,
    pub basal: Vec<BasalSegment>,
    pub alarms: Vec<Alarm>,
}

/// A pump alarm or event. Kinds Glooko names in German are mapped to English.
#[derive(Clone, Debug, PartialEq)]
pub struct Alarm {
    pub at: NaiveDateTime,
    pub kind: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_hypo_treatment_is_carbs_without_insulin() {
        let b = |carbs_g, units| Bolus {
            at: NaiveDateTime::default(),
            carbs_g,
            units,
        };
        assert_eq!(b(60.0, 6.0).kind(), BolusKind::Meal);
        assert_eq!(b(15.0, 0.0).kind(), BolusKind::HypoTreatment);
        assert_eq!(b(0.0, 1.5).kind(), BolusKind::Correction);
        assert_eq!(b(0.0, 0.0).kind(), BolusKind::Correction);
    }
}
