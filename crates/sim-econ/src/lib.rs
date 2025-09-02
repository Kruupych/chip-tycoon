#![deny(warnings)]

//! Economic models: pricing and demand helpers for Chip Tycoon.
//!
//! This module provides validated utilities for:
//! - Optimal monopoly markup under constant elasticity demand
//! - Demand curve evaluation with optional seeded noise
//! - Simple promotional pricing and average selling price (ASP)

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use thiserror::Error;

/// Errors produced by economic helpers.
#[derive(Debug, Error, PartialEq)]
pub enum EconError {
    /// Elasticity must be strictly negative; for monopoly pricing, it must be <= -1.
    #[error("invalid elasticity: {0}")]
    InvalidElasticity(f32),
    /// Monetary values must be non-negative and finite; reference/price must be > 0.
    #[error("invalid price or cost value")]
    InvalidPrice,
    /// Numeric conversion to floating point failed.
    #[error("non-finite numeric conversion")]
    NonFinite,
}

/// Compute a trivial price as cost plus a margin.
///
/// Example:
/// let cost = Decimal::new(100, 2); // 1.00
/// let margin = Decimal::new(50, 2); // 0.50
/// assert_eq!(cost_plus(cost, margin), Decimal::new(150, 2));
pub fn cost_plus(unit_cost: Decimal, margin: Decimal) -> Decimal {
    unit_cost * (Decimal::ONE + margin)
}

/// Optimal monopoly price under constant elasticity demand.
///
/// Uses Lerner index formula: (P - C)/P = -1/ε => P = C / (1 + 1/ε),
/// valid only when ε <= -1. Returns an error otherwise.
///
/// Example:
/// let c = Decimal::new(1000, 2); // 10.00
/// let p = optimal_price(c, -2.0).unwrap();
/// assert!(p > c);
pub fn optimal_price(unit_cost: Decimal, elasticity: f32) -> Result<Decimal, EconError> {
    if !elasticity.is_finite() || elasticity >= -1.0 {
        return Err(EconError::InvalidElasticity(elasticity));
    }
    if unit_cost < Decimal::ZERO {
        return Err(EconError::InvalidPrice);
    }
    // denom in (0, 1) for elasticity <= -1
    let denom = 1.0 + 1.0 / elasticity; // elasticity is negative, denom positive but < 1
    if !(denom.is_finite() && denom > 0.0) {
        return Err(EconError::NonFinite);
    }
    let denom_dec = Decimal::from_f32(denom).ok_or(EconError::NonFinite)?;
    Ok(unit_cost / denom_dec)
}

/// Demand under constant elasticity with respect to a reference price.
///
/// Q = base * (price / ref_price)^{elasticity}. Requires:
/// - base >= 0, price > 0, ref_price > 0, elasticity < 0
/// - Returns non-negative integer quantity (floored), saturating at u64::MAX.
///
/// Example:
/// let q = demand(1000, Decimal::new(100,2), Decimal::new(100,2), -1.5).unwrap();
/// assert_eq!(q, 1000);
pub fn demand(
    base: u64,
    price: Decimal,
    ref_price: Decimal,
    elasticity: f32,
) -> Result<u64, EconError> {
    if !elasticity.is_finite() || elasticity >= 0.0 {
        return Err(EconError::InvalidElasticity(elasticity));
    }
    if price <= Decimal::ZERO || ref_price <= Decimal::ZERO {
        return Err(EconError::InvalidPrice);
    }
    let p = price.to_f64().ok_or(EconError::NonFinite)?;
    let p0 = ref_price.to_f64().ok_or(EconError::NonFinite)?;
    let ratio = p / p0;
    if !(ratio.is_finite() && ratio > 0.0) {
        return Err(EconError::NonFinite);
    }
    let q = (base as f64) * ratio.powf(elasticity as f64);
    if !q.is_finite() || q < 0.0 {
        return Ok(0);
    }
    let qi = q.floor();
    if qi.is_sign_negative() {
        return Ok(0);
    }
    if qi > (u64::MAX as f64) {
        return Ok(u64::MAX);
    }
    Ok(qi as u64)
}

/// Demand with multiplicative uniform noise factor in [1-noise_frac, 1+noise_frac].
///
/// Noise is seeded for reproducibility. `noise_frac` must be in [0, 1).
///
/// Example:
/// let q = demand_with_noise(1000, Decimal::new(1,0), Decimal::new(1,0), -2.0, 0.1, 42).unwrap();
pub fn demand_with_noise(
    base: u64,
    price: Decimal,
    ref_price: Decimal,
    elasticity: f32,
    noise_frac: f32,
    seed: u64,
) -> Result<u64, EconError> {
    if !(0.0..1.0).contains(&noise_frac) || !noise_frac.is_finite() {
        return Err(EconError::NonFinite);
    }
    let q = demand(base, price, ref_price, elasticity)?;
    if noise_frac == 0.0 {
        return Ok(q);
    }
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let u: f32 = rng.gen_range(-noise_frac..=noise_frac);
    let factor = 1.0 + u as f64;
    let noisy = (q as f64) * factor;
    if noisy < 0.0 {
        return Ok(0);
    }
    Ok(noisy.floor().clamp(0.0, u64::MAX as f64) as u64)
}

/// Apply a promotional discount to price. `discount_frac` in [0, 1).
/// Returns discounted price, never negative.
///
/// Example:
/// let p = promo_price(Decimal::new(1000,2), 0.1).unwrap();
/// assert_eq!(p, Decimal::new(900,2));
pub fn promo_price(price: Decimal, discount_frac: f32) -> Result<Decimal, EconError> {
    if price < Decimal::ZERO {
        return Err(EconError::InvalidPrice);
    }
    if !(0.0..1.0).contains(&discount_frac) || !discount_frac.is_finite() {
        return Err(EconError::NonFinite);
    }
    let f = Decimal::from_f32(1.0 - discount_frac).ok_or(EconError::NonFinite)?;
    Ok(price * f)
}

/// Average selling price computed as sum(p_i * q_i) / sum(q_i).
/// Returns None when total quantity is zero.
///
/// Example:
/// let prices = [Decimal::new(100,2), Decimal::new(200,2)];
/// let qty = [1, 1];
/// assert_eq!(asp(&prices, &qty).unwrap(), Decimal::new(150,2));
pub fn asp(prices: &[Decimal], quantities: &[u64]) -> Option<Decimal> {
    if prices.len() != quantities.len() || prices.is_empty() {
        return None;
    }
    let mut num = Decimal::ZERO;
    let mut den: u128 = 0;
    for (p, &q) in prices.iter().zip(quantities) {
        if *p < Decimal::ZERO {
            return None;
        }
        num += *p * Decimal::from(q);
        den = den.saturating_add(q as u128);
    }
    if den == 0 {
        return None;
    }
    let den_dec = Decimal::from(den);
    Some(num / den_dec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_cost_plus() {
        let cost = Decimal::new(100, 2); // 1.00
        let margin = Decimal::new(50, 2); // 0.50
        assert_eq!(cost_plus(cost, margin), Decimal::new(150, 2));
    }

    #[test]
    fn test_optimal_price_basic() {
        let c = Decimal::new(1000, 2); // 10.00
        let p = optimal_price(c, -2.0).unwrap();
        assert!(p > c);
    }

    #[test]
    fn test_optimal_price_invalid_elasticity() {
        let c = Decimal::new(1000, 2);
        assert!(optimal_price(c, -0.5).is_err());
        assert!(optimal_price(c, -1.0).is_err());
        assert!(optimal_price(c, f32::NAN).is_err());
    }

    #[test]
    fn demand_identity_at_ref_price() {
        let q = demand(1000, Decimal::new(100, 2), Decimal::new(100, 2), -2.0).unwrap();
        assert_eq!(q, 1000);
    }

    #[test]
    fn demand_monotonic_decrease_with_price() {
        let base = 1000;
        let p0 = Decimal::new(100, 2);
        let q1 = demand(base, Decimal::new(80, 2), p0, -2.0).unwrap();
        let q2 = demand(base, Decimal::new(120, 2), p0, -2.0).unwrap();
        assert!(q1 > base);
        assert!(q2 < base);
        assert!(q1 > q2);
    }

    #[test]
    fn noise_is_seeded_and_bounded() {
        let base = 1000;
        let p = Decimal::new(100, 2);
        let q1 = demand_with_noise(base, p, p, -2.0, 0.1, 42).unwrap();
        let q2 = demand_with_noise(base, p, p, -2.0, 0.1, 42).unwrap();
        assert_eq!(q1, q2);
        let q3 = demand_with_noise(base, p, p, -2.0, 0.0, 1).unwrap();
        assert_eq!(q3, base);
    }

    #[test]
    fn asp_simple_average() {
        let prices = [Decimal::new(100, 2), Decimal::new(200, 2)];
        let qty = [1, 1];
        assert_eq!(asp(&prices, &qty).unwrap(), Decimal::new(150, 2));
    }

    proptest! {
        #[test]
        fn optimal_price_monotonic_in_cost(cents in 1u64..100_000) {
            let c1 = Decimal::new(cents as i64, 2);
            let c2 = Decimal::new((cents+1) as i64, 2);
            let p1 = optimal_price(c1, -2.0).unwrap();
            let p2 = optimal_price(c2, -2.0).unwrap();
            prop_assert!(p2 > p1);
        }

        #[test]
        fn demand_monotonic(base in 1u64..1_000_000, p in 10i64..10_000, e in -5.0f32..-1.1f32) {
            let price_low = Decimal::new(p as i64, 2);
            let price_high = Decimal::new((p+100) as i64, 2);
            let ref_price = Decimal::new(p as i64, 2);
            let ql = demand(base, price_low, ref_price, e).unwrap();
            let qh = demand(base, price_high, ref_price, e).unwrap();
            prop_assert!(ql >= base);
            prop_assert!(qh <= base);
            prop_assert!(ql >= qh);
        }
    }
}
