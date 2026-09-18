#![warn(missing_docs)]
//! Time-in-range tuner CLI: replay real Glooko data through the verified
//! closed-loop stack (body -> CGM -> aps controller -> pump) and report
//! the clinical outcome metrics.
//!
//! The binary is deliberately thin: it parses Glooko CSVs, builds a
//! [`Scenario`], runs the [`engine`], and prints a metrics line. All the
//! simulation machinery lives in the library so it can be driven from
//! tests and future tooling.
//!
//! For a patient-facing companion page, see `docs/patient/loop.md`.

pub mod engine;
pub mod glooko;

pub use engine::{SimConfig, SimTrace};