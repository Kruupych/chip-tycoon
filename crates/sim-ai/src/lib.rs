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
        .first()
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

// -------------- Horizon planner (beam) --------------

/// Planner inputs derived from the current runtime for the next horizon.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CurrentKpis {
    pub asp_usd: Decimal,
    pub unit_cost_usd: Decimal,
    pub capacity_units_per_month: u64,
    pub cash_usd: Decimal,
    pub debt_usd: Decimal,
    pub share: f32,
    pub rd_progress: f32,
}

/// Planner configuration controlling breadth/depth and economics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerConfig {
    pub beam_width: usize,
    pub months: u32,
    pub quarter_step: u32,
    pub discount: f32,
    pub min_margin_frac: f32,
    pub price_step_frac: f32,
    pub capacity_step_units: u64,
    pub price_pref_beta: f32,
    pub competitor_attractiveness: f32,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            beam_width: 3,
            months: 24,
            quarter_step: 3,
            discount: 0.99,
            min_margin_frac: 0.05,
            price_step_frac: 0.05,
            capacity_step_units: 10_000,
            price_pref_beta: 1.5,
            competitor_attractiveness: 1.0,
        }
    }
}

/// A single action considered by the planner at quarterly decision points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlanAction {
    AdjustPriceFrac(f32),  // +/- fraction of current ASP
    RequestCapacity(u64),  // units/month
    AllocateRndBoost(f32), // +/- boost to R&D progress per month
    ScheduleTapeout { expedite: bool },
    // ScheduleTapeout is omitted in this phase's simplified predictor
}

#[derive(Debug, Clone)]
struct PlannerState {
    asp: Decimal,
    unit_cost: Decimal,
    capacity: u64,
    cash: Decimal,
    debt: Decimal,
    share: f32,
    rd_progress: f32,
    ref_price: Decimal,
}

#[derive(Debug, Clone)]
pub struct PlanStepDecision {
    pub month_index: u32,
    pub action: PlanAction,
}

#[derive(Debug, Clone)]
pub struct PlanResult {
    pub decisions: Vec<PlanStepDecision>,
    pub expected_score: f32,
}

fn price_attractiveness_ratio(asp: Decimal, ref_price: Decimal, beta: f32) -> f32 {
    // A = (ref/price)^beta
    let p = asp.to_f32().unwrap_or(0.0).max(0.01);
    let r = ref_price.to_f32().unwrap_or(p).max(0.01);
    (r / p).powf(beta)
}

fn expected_share_from_price(asp: Decimal, ref_price: Decimal, beta: f32, comp_attr: f32) -> f32 {
    let a = price_attractiveness_ratio(asp, ref_price, beta);
    let denom = a + comp_attr.max(1e-3);
    (a / denom).clamp(0.05, 0.95)
}

fn simulate_month(
    state: &mut PlannerState,
    world: &core::World,
    w: &ScoreWeights,
    cfg: &PlannerConfig,
) -> f32 {
    // Update share based on price attractiveness drifting 10% towards target per month
    let target_share = expected_share_from_price(
        state.asp,
        state.ref_price,
        cfg.price_pref_beta,
        cfg.competitor_attractiveness,
    );
    state.share += (target_share - state.share) * 0.1;
    state.share = state.share.clamp(0.05, 0.95);

    // Market demand at this price
    let seg = world.segments.first();
    let (base_demand, elasticity) = if let Some(s) = seg {
        (s.base_demand_units, s.price_elasticity)
    } else {
        (100_000, -1.2)
    };
    let q_total = sim_econ::demand(base_demand, state.asp, state.ref_price, elasticity)
        .unwrap_or(base_demand);
    // Our addressable demand by share
    let q_our = ((q_total as f32) * state.share).floor() as u64;
    let sell = q_our.min(state.capacity);
    let revenue = state.asp * Decimal::from(sell);
    let cost = state.unit_cost * Decimal::from(sell);
    let profit = revenue - cost;
    state.cash += profit;

    // Utility contribution this month
    let m = CompanyMetrics {
        share_12m: state.share,
        margin_ratio: if revenue > Decimal::ZERO {
            (profit / revenue).to_f32().unwrap_or(0.0).clamp(0.0, 1.0)
        } else {
            0.0
        },
        liquidity_k: safe_ratio(
            state.cash.to_f32().unwrap_or(0.0),
            (state.debt.to_f32().unwrap_or(0.0) + 1.0).max(1.0),
        ),
        portfolio_div: (world.segments.len().max(1) as f32 / 5.0).clamp(0.0, 1.0),
    };
    utility_score(&m, w)
}

fn apply_action(state: &mut PlannerState, action: PlanAction, cfg: &PlannerConfig) {
    match action {
        PlanAction::AdjustPriceFrac(df) => {
            let factor = Decimal::from_f32_retain(1.0 + df).unwrap_or(Decimal::ONE);
            let mut asp = state.asp * factor;
            if !respects_min_margin(asp, state.unit_cost, cfg.min_margin_frac) {
                asp = min_price(state.unit_cost, cfg.min_margin_frac);
            }
            state.asp = asp;
        }
        PlanAction::RequestCapacity(units) => {
            state.capacity = state.capacity.saturating_add(units);
        }
        PlanAction::AllocateRndBoost(boost) => {
            state.rd_progress = (state.rd_progress + boost).clamp(0.0, 1.0);
        }
        PlanAction::ScheduleTapeout { expedite: _ } => {
            // Predictor: slight near-term utility bonus to represent pipeline progress.
            state.rd_progress = (state.rd_progress + 0.005).clamp(0.0, 1.0);
        }
    }
}

/// Run a coarse beam search over the next horizon and return a compact plan.
///
/// This uses a lightweight predictor independent of the main ECS world to keep it fast.
pub fn plan_horizon(
    world: &core::World,
    current: &CurrentKpis,
    w: &ScoreWeights,
    cfg: &PlannerConfig,
) -> PlanResult {
    use std::cmp::Ordering;

    #[derive(Clone)]
    struct Node {
        state: PlannerState,
        score: f32,
        decisions: Vec<PlanStepDecision>,
    }

    let ref_price = current.asp_usd; // treat current as the reference for now
    let init_state = PlannerState {
        asp: current.asp_usd,
        unit_cost: current.unit_cost_usd,
        capacity: current.capacity_units_per_month,
        cash: current.cash_usd,
        debt: current.debt_usd,
        share: current.share.clamp(0.05, 0.95),
        rd_progress: current.rd_progress,
        ref_price,
    };

    let mut beam = vec![Node {
        state: init_state.clone(),
        score: 0.0,
        decisions: vec![],
    }];
    let mut discount_pow = 1.0f32;
    for month in 1..=cfg.months {
        let at_decision = month % cfg.quarter_step == 1; // month 1,4,7,...
        let mut candidates: Vec<Node> = Vec::new();
        if at_decision {
            for n in &beam {
                // Consider a small, curated action set
                let actions: Vec<PlanAction> = if n.state.share < 0.2 {
                    vec![
                        PlanAction::AdjustPriceFrac(-cfg.price_step_frac),
                        PlanAction::AdjustPriceFrac(0.0),
                        PlanAction::ScheduleTapeout { expedite: false },
                        PlanAction::RequestCapacity(cfg.capacity_step_units),
                        PlanAction::AllocateRndBoost(0.01),
                    ]
                } else {
                    vec![
                        PlanAction::AdjustPriceFrac(-cfg.price_step_frac),
                        PlanAction::AdjustPriceFrac(0.0),
                        PlanAction::AdjustPriceFrac(cfg.price_step_frac),
                        PlanAction::ScheduleTapeout { expedite: false },
                        PlanAction::RequestCapacity(cfg.capacity_step_units),
                        PlanAction::AllocateRndBoost(0.01),
                    ]
                };
                for &a in &actions {
                    let mut s = n.state.clone();
                    apply_action(&mut s, a, cfg);
                    let mut s2 = s.clone();
                    let util = simulate_month(&mut s2, world, w, cfg);
                    candidates.push(Node {
                        state: s2,
                        score: n.score + discount_pow * util,
                        decisions: {
                            let mut d = n.decisions.clone();
                            d.push(PlanStepDecision {
                                month_index: month,
                                action: a,
                            });
                            d
                        },
                    });
                }
            }
        } else {
            for n in &beam {
                let mut s2 = n.state.clone();
                let util = simulate_month(&mut s2, world, w, cfg);
                candidates.push(Node {
                    state: s2,
                    score: n.score + discount_pow * util,
                    decisions: n.decisions.clone(),
                });
            }
        }
        // Keep top-k by score
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
        candidates.truncate(cfg.beam_width.max(1));
        beam = candidates;
        discount_pow *= cfg.discount;
    }

    // Return the best plan and its expected score
    let best = beam
        .into_iter()
        .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(Ordering::Equal))
        .unwrap();
    PlanResult {
        decisions: best.decisions,
        expected_score: best.score,
    }
}

#[cfg(test)]
mod planner_tests {
    use super::*;
    use rust_decimal::Decimal;

    fn minimal_world() -> core::World {
        core::World {
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: Decimal::new(10_000_000, 0),
                debt_usd: Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.3,
            }],
        }
    }

    #[test]
    fn low_share_prefers_price_down_not_negative_margin() {
        let world = minimal_world();
        let w = ScoreWeights::default();
        let cfg = PlannerConfig {
            months: 12,
            beam_width: 3,
            price_step_frac: 0.05,
            min_margin_frac: 0.05,
            competitor_attractiveness: 5.0,
            price_pref_beta: 2.0,
            ..Default::default()
        };
        let current = CurrentKpis {
            asp_usd: Decimal::new(300, 0),
            unit_cost_usd: Decimal::new(280, 0),
            capacity_units_per_month: 500_000,
            cash_usd: Decimal::new(1_000_000, 0),
            debt_usd: Decimal::ZERO,
            share: 0.1,
            rd_progress: 0.1,
        };
        let plan = plan_horizon(&world, &current, &w, &cfg);
        // First decision should include a price down or no change, but never cause negative margin
        assert!(!plan.decisions.is_empty());
        let first = &plan.decisions[0];
        if let PlanAction::AdjustPriceFrac(df) = first.action {
            assert!(df <= 0.0); // prefer price down or hold
        }
        // Simulate applying the first decision to check margin floor
        let mut st = PlannerState {
            asp: current.asp_usd,
            unit_cost: current.unit_cost_usd,
            capacity: current.capacity_units_per_month,
            cash: current.cash_usd,
            debt: current.debt_usd,
            share: current.share,
            rd_progress: current.rd_progress,
            ref_price: current.asp_usd,
        };
        apply_action(&mut st, first.action, &cfg);
        let min_price = st.unit_cost * Decimal::from_f32_retain(1.0 + cfg.min_margin_frac).unwrap();
        assert!(st.asp >= min_price);
    }

    #[test]
    fn capacity_shortage_prefers_capacity_over_price_cut() {
        let world = minimal_world();
        let w = ScoreWeights::default();
        let cfg = PlannerConfig {
            months: 12,
            beam_width: 4,
            price_step_frac: 0.05,
            capacity_step_units: 200_000,
            ..Default::default()
        };
        let current = CurrentKpis {
            asp_usd: Decimal::new(300, 0),
            unit_cost_usd: Decimal::new(200, 0),
            capacity_units_per_month: 5_000, // severe shortage vs base demand
            cash_usd: Decimal::new(1_000_000, 0),
            debt_usd: Decimal::ZERO,
            share: 0.4,
            rd_progress: 0.2,
        };
        let plan = plan_horizon(&world, &current, &w, &cfg);
        assert!(!plan.decisions.is_empty());
        let first = &plan.decisions[0];
        match first.action {
            PlanAction::RequestCapacity(_) => {}
            PlanAction::AdjustPriceFrac(df) => {
                // If not capacity, should at least avoid cutting price further under shortage
                assert!(df >= 0.0);
            }
            _ => {}
        }
    }
}

// -------------- Tactics (behavior tree style) --------------

/// Configuration thresholds for monthly tactics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticsConfig {
    /// If share drop over previous month exceeds this delta, reduce price by epsilon.
    pub share_drop_delta: f32,
    /// Magnitude of price change when reacting to share drop.
    pub price_epsilon_frac: f32,
    /// Floor on margin as a fraction of unit cost (e.g., 0.05 = 5%).
    pub min_margin_frac: f32,
    /// If demand/supply ratio exceeds this, raise price by shortage epsilon.
    pub shortage_raise_threshold: f32,
    /// Magnitude of price increase when shortage detected.
    pub shortage_raise_epsilon_frac: f32,
    /// Below this liquidity level, cut R&D by this amount monthly until stable.
    pub cash_liquidity_floor_k: f32,
    /// R&D boost when expediting toward a target (simple stand-in behavior).
    pub rd_boost_on_expedite: f32,
    /// R&D cut applied when liquidity below floor.
    pub rd_cut_on_cash_low: f32,
}

impl Default for TacticsConfig {
    fn default() -> Self {
        Self {
            share_drop_delta: 0.05,
            price_epsilon_frac: 0.02,
            min_margin_frac: 0.05,
            shortage_raise_threshold: 1.2,
            shortage_raise_epsilon_frac: 0.02,
            cash_liquidity_floor_k: 0.5,
            rd_boost_on_expedite: 0.01,
            rd_cut_on_cash_low: 0.01,
        }
    }
}

/// Decide monthly tactics: price delta and R&D adjustment.
/// Returns (price_delta_frac, rd_boost_delta).
pub fn decide_tactics(
    metrics: &CompanyMetrics,
    last_share: f32,
    demand_supply_ratio: f32,
    unit_cost: Decimal,
    asp: Decimal,
    cfg: &TacticsConfig,
) -> (f32, f32) {
    let mut price_df = 0.0f32;
    let mut rd_boost = 0.0f32;

    let share_drop = (last_share - metrics.share_12m).max(0.0);
    if share_drop > cfg.share_drop_delta {
        price_df -= cfg.price_epsilon_frac;
    }
    if demand_supply_ratio > cfg.shortage_raise_threshold {
        price_df += cfg.shortage_raise_epsilon_frac;
    }
    // Enforce margin floor: if price cut would violate, clamp delta to floor
    if price_df < 0.0 {
        let new_price =
            asp * rust_decimal::Decimal::from_f32_retain(1.0 + price_df).unwrap_or(Decimal::ONE);
        if !respects_min_margin(new_price, unit_cost, cfg.min_margin_frac) {
            // Compute maximal allowed negative delta
            let minp = min_price(unit_cost, cfg.min_margin_frac);
            let max_negative = (minp / asp).to_f32().unwrap_or(1.0) - 1.0;
            price_df = max_negative.max(0.0);
        }
    }

    // R&D policy: expedite if margin is healthy and share lags; cut if liquidity low.
    if metrics.liquidity_k < cfg.cash_liquidity_floor_k {
        rd_boost -= cfg.rd_cut_on_cash_low;
    } else if share_drop > cfg.share_drop_delta {
        rd_boost += cfg.rd_boost_on_expedite;
    }

    (price_df, rd_boost)
}

/// AI config with weights, planner, and tactics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiConfig {
    pub weights: ScoreWeights,
    pub planner: PlannerConfig,
    pub tactics: TacticsConfig,
    pub product_weights: ProductWeights,
    pub product_cost: ProductCostCfg,
}

/// Default YAML baked in from the assets directory.
pub const AI_DEFAULTS_YAML: &str = include_str!("../../../assets/data/ai_defaults.yaml");

impl AiConfig {
    pub fn from_default_yaml() -> Result<AiConfig, serde_yaml::Error> {
        serde_yaml::from_str(AI_DEFAULTS_YAML)
    }
}

/// Weights for product attractiveness in sales.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductWeights {
    pub perf: f32,
    pub price_rel: f32,
    pub appeal: f32,
}

impl Default for ProductWeights {
    fn default() -> Self {
        Self {
            perf: 0.7,
            price_rel: 0.0,
            appeal: 0.3,
        }
    }
}

/// Parameters for unit-cost computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductCostCfg {
    pub usable_die_area_mm2: f32,
    pub yield_overhead_frac: f32,
}

impl Default for ProductCostCfg {
    fn default() -> Self {
        Self {
            usable_die_area_mm2: 6200.0,
            yield_overhead_frac: 0.05,
        }
    }
}

/// Shared min-margin helpers
pub fn min_price(unit_cost: Decimal, min_margin_frac: f32) -> Decimal {
    let f = rust_decimal::Decimal::from_f32_retain(1.0 + min_margin_frac).unwrap_or(Decimal::ONE);
    unit_cost * f
}

pub fn respects_min_margin(asp: Decimal, unit_cost: Decimal, min_margin_frac: f32) -> bool {
    asp >= min_price(unit_cost, min_margin_frac)
}

#[cfg(test)]
mod tactics_tests {
    use super::*;

    #[test]
    fn reduces_price_on_share_drop_with_floor() {
        let cfg = TacticsConfig {
            share_drop_delta: 0.05,
            price_epsilon_frac: 0.1,
            min_margin_frac: 0.05,
            ..Default::default()
        };
        let unit_cost = Decimal::new(200, 0);
        let asp = Decimal::new(220, 0); // current margin ~9.09%
        let m = CompanyMetrics {
            share_12m: 0.2,
            margin_ratio: 0.09,
            liquidity_k: 1.0,
            portfolio_div: 0.5,
        };
        let (df, _rd) = decide_tactics(&m, 0.3, 1.0, unit_cost, asp, &cfg);
        // requested cut 10% would violate min margin 5%
        // floor should limit it to ~ (210/220 - 1) = -0.04545...
        assert!(df <= 0.0);
        let new_price = asp * rust_decimal::Decimal::from_f32_retain(1.0 + df).unwrap();
        let min_price =
            unit_cost * rust_decimal::Decimal::from_f32_retain(1.0 + cfg.min_margin_frac).unwrap();
        assert!(new_price >= min_price);
    }

    #[test]
    fn increases_price_on_shortage() {
        let cfg = TacticsConfig {
            shortage_raise_threshold: 1.2,
            shortage_raise_epsilon_frac: 0.03,
            ..Default::default()
        };
        let unit_cost = Decimal::new(200, 0);
        let asp = Decimal::new(300, 0);
        let m = CompanyMetrics {
            share_12m: 0.3,
            margin_ratio: 0.33,
            liquidity_k: 1.0,
            portfolio_div: 0.5,
        };
        let (df, _rd) = decide_tactics(&m, 0.3, 1.5, unit_cost, asp, &cfg);
        assert!(df > 0.0);
    }
}
