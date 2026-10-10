//! Recorded therapy data: what the pump and CGM logged, in local time.
#![allow(missing_docs)]

use chrono::{NaiveDate, NaiveDateTime};

/// A CGM sample or fingerstick reading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlucoseReading {
    pub at: NaiveDateTime,
    pub mg_dl: f64,
}

/// A manual bolus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bolus {
    pub at: NaiveDateTime,
    pub carbs_g: f64,
    pub units: f64,
}

impl Bolus {
    /// A bolus without carbs is a correction.
    pub fn is_meal(&self) -> bool {
        self.carbs_g > 0.0
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
