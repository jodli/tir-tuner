//! Shared primitives for the tir-tuner workspace.
//!
//! Everything here is algorithm-independent: numeric integration
//! primitives, unit conversions and clinical reporting metrics. The
//! [`tir_tuner_aps`] crate (the verified pump algorithm) and the
//! `tir-tuner-body` / `tir-tuner-cgm` simulation crates all build on it
//! without sharing semantics.

pub mod euler;
pub mod metrics;
pub mod random;
pub mod units;

#[cfg(kani)]
pub mod verification;