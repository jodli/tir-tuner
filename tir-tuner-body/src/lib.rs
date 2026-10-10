#![warn(missing_docs)]
//! The virtual patient of Wilinska et al. 2010, with Fiasp absorption and
//! the YpsoPump's bolus speed.
//!
//! Eleven compartments: subcutaneous insulin `s1`/`s2`, plasma insulin `i`,
//! insulin actions `x1`-`x3`, glucose masses `q1`/`q2`, gut `g1`/`g2` and
//! interstitial glucose `c`. EGP follows Hovorka 2004 eq 1 where W10 has a
//! sign typo. Every number traces to `docs/sources.toml`.
//!
//! The crate doesn't depend on the controller; the loop lives in
//! `tir-tuner-cli`. Patient-facing page: `docs/patient/body.md`.

pub mod derivative;
pub mod pump;
pub mod solver;
pub mod state;
pub mod subject;

#[cfg(kani)]
pub mod verification;
