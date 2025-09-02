#![deny(warnings)]

//! Core domain models and invariants for Chip Tycoon.
//! This crate defines serializable types used across the simulation.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Unique identifier for a technology node, e.g. "800nm", "N7", "N5", "2nm".
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TechNodeId(pub String);

/// Simulation configuration parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimConfig {
    /// Number of days per tick (default: 30 for monthly).
    pub tick_days: u16,
    /// Seed for deterministic RNG.
    pub rng_seed: u64,
}

/// Macro-economic state for a given date.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MacroState {
    pub date: NaiveDate,
    pub inflation_annual: f32,
    pub interest_rate: f32,
    pub fx_usd_index: f32,
}

/// Example invariant check: inflation and rates are finite.
pub fn validate_macro_state(m: &MacroState) -> bool {
    m.inflation_annual.is_finite() && m.interest_rate.is_finite() && m.fx_usd_index.is_finite()
}

/// A trivial function used by tests to avoid unused warnings.
pub fn add_decimal(a: Decimal, b: Decimal) -> Decimal {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_add_decimal() {
        let a = Decimal::new(10, 0);
        let b = Decimal::new(5, 0);
        assert_eq!(add_decimal(a, b), Decimal::new(15, 0));
    }
}
