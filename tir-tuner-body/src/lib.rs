#![warn(missing_docs)]
//! Physiological virtual-patient model (Wilinska et al. 2010) for
//! in-silico closed-loop testing.
//!
//! The model integrates eleven compartments that mirror the Cambridge
//! simulator subject: subcutaneous insulin depots `s1`/`s2`, plasma
//! insulin `i`, the three insulin actions `x1`/`x2`/`x3`, glucose masses
//! `q1`/`q2`, the gut absorption chain `g1`/`g2`, and the interstitial
//! glucose `c` that a sensor would observe. ODEs are given in
//! [`derivative`] and integrated with a clamped forward Euler step
//! (see [`solver`]) so every compartment provably stays non-negative.
//!
//! This crate is deliberately independent of the pump algorithm crate
//! [`tir_tuner_aps`]: the body only consumes insulin and produces
//! interstitial glucose, the closed loop that connects both lives in the
//! `tir-tuner-cli` crate.
//!
//! Known model divergences from the published Wilinska formulation are
//! documented where they occur in [`subject`].
//!
//! For a patient-facing companion page, see `docs/patient/body.md`.

pub mod derivative;
pub mod solver;
pub mod state;
pub mod subject;

#[cfg(kani)]
pub mod verification;