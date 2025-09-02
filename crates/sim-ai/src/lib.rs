#![deny(warnings)]

//! AI policies for Chip Tycoon: utility scoring, horizon planner, and tactics.
//!
//! The AI in this phase focuses on three pieces:
//! - Utility scoring: converts company metrics into a scalar in [0,1].
//! - Horizon planner: a fast, coarse beam search over quarterly actions.
//! - Tactics: month-to-month behavior for price and R&D knobs.

use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use sim_core as core;

/// Weights for the utility components.
///
/// Defaults are tuned for balanced behavior:
/// share=0.4, margin=0.3, liquidity=0.2, portfolio=0.1
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ScoreWeights {
    pub share: f32,
    pub margin: f32,
    pub liquidity: f32,
    pub portfolio: f32,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            share: 0.4,
            margin: 0.3,
            liquidity: 0.2,
            portfolio: 0.1,
        }
    }
}

/// Minimal metrics required by the AI scoring.
///
/// This structure intentionally uses fields we can approximate deterministically
/// from the current `sim-runtime` resources without adding heavy state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompanyMetrics {
    /// Market share in [0,1] over last ~12 months (approximation OK).
    pub share_12m: f32,
    /// Gross margin ratio in [0,1], e.g. 0.25 for 25%.
    pub margin_ratio: f32,
    /// Liquidity proxy in [0, +inf): cash / (debt + 1). Will be normalized internally.
    pub liquidity_k: f32,
    /// Portfolio diversification in [0,1] (e.g. entropy proxy or simple spread).
    pub portfolio_div: f32,
}

fn norm01(x: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    x.clamp(0.0, 1.0)
}

fn safe_ratio(num: f32, den: f32) -> f32 {
    if !num.is_finite() || !den.is_finite() {
        return 0.0;
    }
    if den.abs() < 1e-9 {
        0.0
    } else {
        (num / den).clamp(-1e6, 1e6)
    }
}

/// Normalize liquidity proxy into [0,1] via smooth clipping.
fn norm_liquidity(liq_k: f32) -> f32 {
    if !liq_k.is_finite() {
        return 0.0;
    }
    // Map k in [0, 5] -> [0,1], saturate above 5
    (liq_k / 5.0).clamp(0.0, 1.0)
}

/// Compute utility score in [0,1] given company metrics and component weights.
///
/// Behavior:
/// - Inputs are individually clamped/normalized to [0,1].
/// - Non-finite values are treated conservatively (as zeros) to avoid NaNs.
/// - The weighted sum is normalized by the sum of weights if they don't add to 1.
pub fn utility_score(metrics: &CompanyMetrics, w: &ScoreWeights) -> f32 {
    let share = norm01(metrics.share_12m);
    let margin = norm01(metrics.margin_ratio);
    let liquidity = norm_liquidity(metrics.liquidity_k);
    let portfolio = norm01(metrics.portfolio_div);

    let mut ws = [w.share, w.margin, w.liquidity, w.portfolio];
    for wi in &mut ws {
        if !wi.is_finite() || *wi < 0.0 {
            *wi = 0.0;
        }
    }
    let wsum = ws.iter().sum::<f32>();
    let wnorm = if wsum > 0.0 { 1.0 / wsum } else { 0.0 };
    let score = share * ws[0] + margin * ws[1] + liquidity * ws[2] + portfolio * ws[3];
    (score * wnorm).clamp(0.0, 1.0)
}

/// Construct `CompanyMetrics` from currently available domain/runtime info.
///
/// This is a lightweight adapter intended for the current simplified runtime.
/// - share_12m: provided by caller or derived from runtime stats.
/// - margin_ratio: approximated from revenue/profit if ASP is unknown.
/// - liquidity_k: computed from first company's cash/debt.
/// - portfolio_div: simple proxy based on number of segments (more segments => higher).
pub fn metrics_from_world(
    world: &core::World,
    share_12m: f32,
    revenue_usd: Decimal,
    profit_usd: Decimal,
) -> CompanyMetrics {
    let revenue_f = revenue_usd.to_f32().unwrap_or(0.0);
    let profit_f = profit_usd.to_f32().unwrap_or(0.0);
    let margin_ratio = if revenue_f > 0.0 {
        safe_ratio(profit_f, revenue_f).clamp(0.0, 1.0)
    } else {
        0.3
    };
    let (cash, debt) = world
        .companies
        .get(0)
        .map(|c| (c.cash_usd, c.debt_usd))
        .unwrap_or((Decimal::ZERO, Decimal::ZERO));
    let liq_k = safe_ratio(
        cash.to_f32().unwrap_or(0.0),
        (debt.to_f32().unwrap_or(0.0) + 1.0).max(1.0),
    )
    .max(0.0);
    let segs = world.segments.len().max(1) as f32;
    let portfolio_div = (segs / 5.0).clamp(0.0, 1.0);

    CompanyMetrics {
        share_12m,
        margin_ratio,
        liquidity_k: liq_k,
        portfolio_div,
    }
}

// -------------- Tests for utility scoring --------------

#[cfg(test)]
mod tests {
    use super::*;

    fn weights() -> ScoreWeights {
        ScoreWeights::default()
    }

    #[test]
    fn monotonic_in_share_and_margin() {
        let w = weights();
        let base = CompanyMetrics {
            share_12m: 0.2,
            margin_ratio: 0.2,
            liquidity_k: 0.5,
            portfolio_div: 0.3,
        };
        let mut up_share = base.clone();
        up_share.share_12m = 0.3;
        let mut up_margin = base.clone();
        up_margin.margin_ratio = 0.3;
        assert!(utility_score(&up_share, &w) >= utility_score(&base, &w));
        assert!(utility_score(&up_margin, &w) >= utility_score(&base, &w));
    }

    #[test]
    fn stability_same_inputs_same_score() {
        let w = weights();
        let m = CompanyMetrics {
            share_12m: 0.4,
            margin_ratio: 0.25,
            liquidity_k: 1.5,
            portfolio_div: 0.6,
        };
        let s1 = utility_score(&m, &w);
        let s2 = utility_score(&m, &w);
        assert!((s1 - s2).abs() < 1e-9);
    }
}
