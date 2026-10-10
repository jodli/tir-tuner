#![warn(missing_docs)]
//! Shared primitives for the tir-tuner workspace.
//!
//! Everything here is algorithm-independent: numeric integration
//! primitives, unit conversions, clinical reporting metrics and the
//! domain types for recorded therapy data. The
//! [`tir_tuner_aps`] crate (the verified pump algorithm) and the
//! `tir-tuner-body` / `tir-tuner-cgm` simulation crates all build on it
//! without sharing semantics.
//!
//! For a patient-facing companion page, see `docs/patient/math-and-metrics.md`.

pub mod data;
pub mod euler;
pub mod metrics;
pub mod random;
pub mod settings;
pub mod units;

#[cfg(kani)]
pub mod verification;
