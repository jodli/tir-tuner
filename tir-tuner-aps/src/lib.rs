#![warn(missing_docs)]
//! The controller side of Auto mode, aligned with what Cambridge published:
//! the model of US8977504B2, the NMPC of Hovorka 2004, Boost and Ease-off
//! as Ware 2022/2022b describe them. CamAPS FX itself isn't published, so
//! this is the published lineage, not the product.
//!
//! * [`hovorka`]: the patent's glucoregulatory model.
//! * [`controller`]: eq 9 NMPC, the Auto mode variants, the hypo cutoff.
//! * [`kalman`]: the patent's IMM filter, eq 53-82.
//! * [`imm`]: its mode-probability arithmetic, eq 66-67 and 80.
//! * [`AutoMode`]: personal target and weight in the controller's units.
//!
//! Kani proofs live in [`verification`] (`cargo kani`).

pub mod controller;
pub mod hovorka;
pub mod imm;
pub mod kalman;
mod settings;

pub use settings::AutoMode;

#[cfg(kani)]
pub mod verification;

/// Shared configuration for the native `proptest` property suite.
#[cfg(test)]
pub(crate) mod test_support {
    use proptest::prelude::ProptestConfig;

    /// Property-test configuration. `TIR_TUNER_SOAK_ITERS` raises the case
    /// count for the opt-in soak run; the default keeps `cargo test`
    /// well under the fast budget.
    pub fn config() -> ProptestConfig {
        let mut config = ProptestConfig::default();
        if let Ok(value) = std::env::var("TIR_TUNER_SOAK_ITERS")
            && let Ok(cases) = value.parse::<u32>()
        {
            config.cases = cases;
        }
        config
    }
}
