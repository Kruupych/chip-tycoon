#![deny(warnings)]

//! Core domain models and invariants for Chip Tycoon.
//!
//! This crate defines serializable types used across the simulation with
//! validation helpers to guarantee basic invariants.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Unique identifier for a technology node, e.g. "800nm", "N7", "N5", "2nm".
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TechNodeId(pub String);

/// A fabrication technology node with cost and physical characteristics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TechNode {
    /// Node identifier, e.g. "N5".
    pub id: TechNodeId,
    /// First year the node becomes available.
    pub year_available: i32,
    /// Transistor density in MTr per mm².
    pub density_mtr_per_mm2: Decimal,
    /// Baseline achievable frequency in GHz.
    pub freq_ghz_baseline: Decimal,
    /// Relative leakage index (dimensionless, >= 0).
    pub leakage_index: Decimal,
    /// Baseline die yield in [0,1].
    pub yield_baseline: Decimal,
    /// Wafer cost in USD.
    pub wafer_cost_usd: Decimal,
    /// Mask set cost in USD.
    pub mask_set_cost_usd: Decimal,
    /// Prerequisite nodes that must exist/be unlocked.
    pub dependencies: Vec<TechNodeId>,
}

/// Kinds of semiconductor products.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProductKind {
    /// Central Processing Unit
    CPU,
    /// Graphics Processing Unit
    GPU,
    /// Accelerated Processing Unit (CPU+GPU)
    APU,
    /// Application-Specific Integrated Circuit
    ASIC,
    /// Neural Processing Unit
    NPU,
}

/// Micro-architecture characteristics that affect performance/cost.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MicroArch {
    /// Relative IPC index (dimensionless, > 0).
    pub ipc_index: f32,
    /// Pipeline depth in stages (> 0).
    pub pipeline_depth: u8,
    /// L1 cache size (KB).
    pub cache_l1_kb: u16,
    /// L2 cache size (MB).
    pub cache_l2_mb: f32,
    /// Whether design uses chiplets.
    pub chiplet: bool,
}

/// A specific product specification for manufacturing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductSpec {
    /// Product kind.
    pub kind: ProductKind,
    /// Target technology node ID.
    pub tech_node: TechNodeId,
    /// Micro-architectural parameters.
    pub microarch: MicroArch,
    /// Die area in mm² (> 0).
    pub die_area_mm2: f32,
    /// Thermal Design Power in Watts (>= 0).
    pub tdp_w: f32,
    /// Bill of materials cost in USD (>= 0).
    pub bom_usd: f32,
}

/// Macro-economic state for a given date.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MacroState {
    /// Current simulation date.
    pub date: NaiveDate,
    /// Annual inflation (e.g., 0.02 = 2%).
    pub inflation_annual: f32,
    /// Short-term interest rate (e.g., 0.05).
    pub interest_rate: f32,
    /// USD FX index (baseline ~100).
    pub fx_usd_index: f32,
}

/// A targetable market segment with demand characteristics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketSegment {
    /// Human-readable segment name (e.g., "Desktop CPU").
    pub name: String,
    /// Baseline demand in units per tick.
    pub base_demand_units: u64,
    /// Price elasticity (< 0 for standard goods).
    pub price_elasticity: f32,
}

/// Simulation configuration parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimConfig {
    /// Number of days per tick (default: 30 for monthly).
    pub tick_days: u16,
    /// Seed for deterministic RNG.
    pub rng_seed: u64,
}

/// Minimal representation of a company participating in the simulation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Company {
    /// Company brand name.
    pub name: String,
    /// Cash reserves in USD (>= 0 for baseline setup).
    pub cash_usd: Decimal,
    /// Outstanding debt in USD (>= 0).
    pub debt_usd: Decimal,
    /// Owned IP tags (placeholder for future modeling).
    pub ip_portfolio: Vec<String>,
}

/// Top-level world state with technology, companies, and market data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct World {
    /// Macro-economic state.
    pub macro_state: MacroState,
    /// Technology tree (unique IDs).
    pub tech_tree: Vec<TechNode>,
    /// Participating companies.
    pub companies: Vec<Company>,
    /// Addressable market segments.
    pub segments: Vec<MarketSegment>,
}

/// Validation errors for domain invariants.
#[derive(Debug, Error, PartialEq)]
pub enum ValidationError {
    /// Year outside supported range [1970, 2100].
    #[error("year {0} is out of supported range [1970, 2100]")]
    YearOutOfRange(i32),
    /// Yield must be within [0, 1].
    #[error("yield must be within [0,1]")]
    InvalidYield,
    /// Numeric field must be finite.
    #[error("non-finite numeric value encountered")]
    NonFinite,
    /// Price or cost must be non-negative.
    #[error("negative monetary value is invalid")]
    NegativeMoney,
    /// Area must be strictly positive.
    #[error("die area must be > 0")]
    NonPositiveArea,
    /// Elasticity must be strictly negative.
    #[error("price elasticity must be < 0")]
    ElasticityNonNegative,
    /// Missing dependency in tech tree.
    #[error("dependency not found: {0}")]
    DependencyNotFound(String),
}

/// Validate a technology node.
pub fn validate_tech_node(node: &TechNode) -> Result<(), ValidationError> {
    if !(1970..=2100).contains(&node.year_available) {
        return Err(ValidationError::YearOutOfRange(node.year_available));
    }
    if node.yield_baseline < Decimal::ZERO || node.yield_baseline > Decimal::ONE {
        return Err(ValidationError::InvalidYield);
    }
    if node.wafer_cost_usd < Decimal::ZERO || node.mask_set_cost_usd < Decimal::ZERO {
        return Err(ValidationError::NegativeMoney);
    }
    if node.density_mtr_per_mm2 <= Decimal::ZERO || node.leakage_index < Decimal::ZERO {
        return Err(ValidationError::NonFinite);
    }
    if node.freq_ghz_baseline < Decimal::ZERO {
        return Err(ValidationError::NonFinite);
    }
    Ok(())
}

/// Validate a micro-architecture.
pub fn validate_microarch(m: &MicroArch) -> Result<(), ValidationError> {
    if !m.ipc_index.is_finite() || !m.cache_l2_mb.is_finite() {
        return Err(ValidationError::NonFinite);
    }
    if m.ipc_index <= 0.0 || m.pipeline_depth == 0 {
        return Err(ValidationError::NonFinite);
    }
    Ok(())
}

/// Validate a product specification.
pub fn validate_product_spec(p: &ProductSpec) -> Result<(), ValidationError> {
    validate_microarch(&p.microarch)?;
    if p.die_area_mm2 <= 0.0 {
        return Err(ValidationError::NonPositiveArea);
    }
    if p.tdp_w < 0.0 || p.bom_usd < 0.0 {
        return Err(ValidationError::NegativeMoney);
    }
    Ok(())
}

/// Validate a market segment.
pub fn validate_segment(s: &MarketSegment) -> Result<(), ValidationError> {
    if s.name.trim().is_empty() {
        return Err(ValidationError::NonFinite);
    }
    if !s.price_elasticity.is_finite() {
        return Err(ValidationError::NonFinite);
    }
    if s.price_elasticity >= 0.0 {
        return Err(ValidationError::ElasticityNonNegative);
    }
    Ok(())
}

/// Validate macro-economic state fields.
pub fn validate_macro_state(m: &MacroState) -> Result<(), ValidationError> {
    if !(m.inflation_annual.is_finite()
        && m.interest_rate.is_finite()
        && m.fx_usd_index.is_finite())
    {
        return Err(ValidationError::NonFinite);
    }
    Ok(())
}

/// Validate the world, including cross-references like tech dependencies.
pub fn validate_world(world: &World) -> Result<(), ValidationError> {
    validate_macro_state(&world.macro_state)?;
    for s in &world.segments {
        validate_segment(s)?;
    }
    for c in &world.companies {
        if c.name.trim().is_empty() {
            return Err(ValidationError::NonFinite);
        }
        if c.cash_usd < Decimal::ZERO || c.debt_usd < Decimal::ZERO {
            return Err(ValidationError::NegativeMoney);
        }
    }

    let mut ids: BTreeSet<&TechNodeId> = BTreeSet::new();
    let mut id_map: BTreeMap<&TechNodeId, &TechNode> = BTreeMap::new();
    for n in &world.tech_tree {
        validate_tech_node(n)?;
        if !ids.insert(&n.id) {
            // Duplicate IDs would be an issue later; treat as dependency error for now.
            return Err(ValidationError::DependencyNotFound(n.id.0.clone()));
        }
        id_map.insert(&n.id, n);
    }
    for n in &world.tech_tree {
        for dep in &n.dependencies {
            if !id_map.contains_key(dep) {
                return Err(ValidationError::DependencyNotFound(dep.0.clone()));
            }
        }
    }
    Ok(())
}

/// A trivial function used by tests to avoid unused warnings in minimal setups.
pub fn add_decimal(a: Decimal, b: Decimal) -> Decimal {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal::Decimal;

    fn node(id: &str) -> TechNode {
        TechNode {
            id: TechNodeId(id.to_string()),
            year_available: 2000,
            density_mtr_per_mm2: Decimal::new(100, 0),
            freq_ghz_baseline: Decimal::new(4, 0),
            leakage_index: Decimal::new(1, 0),
            yield_baseline: Decimal::new(9, 1), // 0.9
            wafer_cost_usd: Decimal::new(1000, 0),
            mask_set_cost_usd: Decimal::new(5000, 0),
            dependencies: vec![],
        }
    }

    #[test]
    fn serde_roundtrip_technode() {
        let n = node("N7");
        let s = serde_json::to_string(&n).unwrap();
        let back: TechNode = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id.0, "N7");
        assert_eq!(back.year_available, 2000);
    }

    #[test]
    fn world_snapshot_roundtrip() {
        let world = World {
            macro_state: MacroState {
                date: NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![node("800nm"), node("N7")],
            companies: vec![Company {
                name: "TestCo".to_string(),
                cash_usd: Decimal::new(1_000_000, 0),
                debt_usd: Decimal::new(0, 0),
                ip_portfolio: vec!["uArchX".to_string()],
            }],
            segments: vec![MarketSegment {
                name: "Desktop CPU".to_string(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.2,
            }],
        };
        validate_world(&world).unwrap();
        let s = serde_json::to_string_pretty(&world).unwrap();
        let back: World = serde_json::from_str(&s).unwrap();
        assert_eq!(back.tech_tree.len(), 2);
        assert_eq!(back.companies.len(), 1);
        assert_eq!(back.segments.len(), 1);
    }

    proptest! {
        #[test]
        fn product_area_positive(area in 0.1f32..10_000.0,
                                 tdp in 0.0f32..500.0,
                                 bom in 0.0f32..100_000.0) {
            let p = ProductSpec {
                kind: ProductKind::CPU,
                tech_node: TechNodeId("N7".to_string()),
                microarch: MicroArch { ipc_index: 1.0, pipeline_depth: 10, cache_l1_kb: 64, cache_l2_mb: 1.0, chiplet: false },
                die_area_mm2: area,
                tdp_w: tdp,
                bom_usd: bom,
            };
            prop_assert!(validate_product_spec(&p).is_ok());
        }

        #[test]
        fn elasticity_is_negative(e in -5.0f32..-0.001f32) {
            let s = MarketSegment { name: "Seg".to_string(), base_demand_units: 1000, price_elasticity: e };
            prop_assert!(validate_segment(&s).is_ok());
        }
    }

    #[test]
    fn test_add_decimal() {
        let a = Decimal::new(10, 0);
        let b = Decimal::new(5, 0);
        assert_eq!(add_decimal(a, b), Decimal::new(15, 0));
    }
}
