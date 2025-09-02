#![deny(warnings)]

//! ECS runtime for the simulation.
//!
//! Exposes a simple monthly tick runner with deterministic, stubbed systems.

use bevy_ecs::prelude::*;
use chrono::Datelike;
use chrono::NaiveDate;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rust_decimal::{
    prelude::{FromPrimitive, ToPrimitive},
    Decimal,
};
use sim_ai as ai;
use sim_core as core;
use tracing::info;

/// Resource wrapper for domain world state.
#[derive(Resource)]
pub struct DomainWorld(pub core::World);

/// Resource for simulation configuration.
#[derive(Resource, Clone)]
pub struct SimConfig(pub core::SimConfig);

/// Resource accumulating KPI-like stats across ticks.
#[derive(Resource, Default, Clone)]
pub struct Stats {
    pub months_run: u32,
    pub revenue_usd: Decimal,
    pub profit_usd: Decimal,
    pub cogs_usd: Decimal,
    pub contract_costs_cents: i64,
    pub market_share: f32,
    pub last_share: f32,
    pub rd_progress: f32,
    pub last_sold_units: u64,
    pub output_units: u64,
    pub defect_units: u64,
    pub inventory_units: u64,
}

/// Snapshot of aggregated KPIs after running the simulation.
#[derive(Clone, Debug)]
pub struct SimSnapshot {
    pub months_run: u32,
    pub cash_cents: i64,
    pub revenue_cents: i64,
    pub cogs_cents: i64,
    pub profit_cents: i64,
    pub contract_costs_cents: i64, // already in cents
    pub asp_cents: i64,
    pub unit_cost_cents: i64,
    pub market_share: f32,
    pub rd_progress: f32,
    pub output_units: u64,
    pub defect_units: u64,
    pub inventory_units: u64,
}

/// Per-month telemetry captured after each tick.
#[derive(Clone, Debug, Default)]
pub struct MonthlyTelemetry {
    pub month_index: u32,
    pub output_units: u64,
    pub sold_units: u64,
    pub asp_usd: Decimal,
    pub unit_cost_usd: Decimal,
    pub margin_usd: Decimal,
    pub revenue_usd: Decimal,
}

/// Pricing resource to allow AI to adjust ASP while sales reads it.
#[derive(Resource, Clone)]
pub struct Pricing {
    pub asp_usd: Decimal,
    pub unit_cost_usd: Decimal,
}

/// Simple product appeal metric influenced by released tapeouts.
#[derive(Resource, Default, Clone)]
pub struct ProductAppeal(pub f32);

/// Product pipeline resource wraps core pipeline.
#[derive(Resource, Default, Clone)]
pub struct Pipeline(pub core::ProductPipeline);

/// Active product characteristics used in sales attractiveness.
#[derive(Resource, Default, Clone)]
pub struct ActiveProduct {
    pub perf_index: f32,
}

impl Default for Pricing {
    fn default() -> Self {
        Self {
            asp_usd: Decimal::new(300, 0),
            unit_cost_usd: Decimal::new(200, 0),
        }
    }
}

/// R&D system: increases R&D progress deterministically.
pub fn r_and_d_system(mut stats: ResMut<Stats>) {
    let inc = 0.01f32 + stats_rd_boost(&stats); // baseline + policy boost
    stats.rd_progress = (stats.rd_progress + inc).clamp(0.0, 1.0);
    info!(target: "sim.rnd", rd_progress = stats.rd_progress, "R&D progress updated");
}

/// Foundry capacity: placeholder system to influence production.
#[derive(Resource, Default)]
pub struct Capacity {
    pub wafers_per_month: u64,
}

/// Player-controlled monthly R&D budget in cents.
#[derive(Resource, Default, Clone, Copy)]
pub struct RnDBudgetCents(pub i64);

/// Global RNG resource seeded from `SimConfig` for deterministic noise.
#[derive(Resource)]
pub struct RngResource(pub ChaCha8Rng);

/// Foundry capacity contracts.
#[derive(Clone, Debug)]
pub struct FoundryContract {
    pub foundry_id: String,
    pub wafers_per_month: u32,
    pub price_per_wafer_cents: i64,
    pub take_or_pay_frac: f32,
    pub billing_cents_per_wafer: i64,
    pub billing_model: &'static str, // "take_or_pay" | "pay_as_used"
    pub lead_time_months: u8,
    pub start: chrono::NaiveDate,
    pub end: chrono::NaiveDate,
}

/// Capacity book resource with active/pending contracts.
#[derive(Resource, Default, Clone, Debug)]
pub struct CapacityBook {
    pub contracts: Vec<FoundryContract>,
}

pub fn foundry_capacity_system(
    mut cap: ResMut<Capacity>,
    dom: Res<DomainWorld>,
    book: Res<CapacityBook>,
) {
    // Base capacity from world size
    let base = 1000u64;
    let factor = (dom.0.tech_tree.len() as u64 + dom.0.companies.len() as u64).max(1);
    let mut wafers = base * factor;
    // Add active contracts effective at current date
    let date = dom.0.macro_state.date;
    for c in &book.contracts {
        if date >= c.start && date <= c.end {
            wafers = wafers.saturating_add(c.wafers_per_month as u64);
        }
    }
    cap.wafers_per_month = wafers;
    info!(target: "sim.capacity", wafers = cap.wafers_per_month, "Capacity calculated");
}

/// Production system: converts capacity into output and defects.
pub fn production_system(mut stats: ResMut<Stats>, cap: Res<Capacity>) {
    let produced = cap.wafers_per_month * 50; // 50 dies per wafer (dummy)
    let defects = produced / 20; // 5% defects (dummy)
    let good = produced.saturating_sub(defects);
    stats.output_units = stats.output_units.saturating_add(good);
    stats.defect_units = stats.defect_units.saturating_add(defects);
    stats.inventory_units = stats.inventory_units.saturating_add(good);
    info!(target: "sim.prod", good, defects, inv = stats.inventory_units, "Production executed");
}

/// Sales system: sells some inventory weighted by product attractiveness.
pub fn sales_system(
    mut stats: ResMut<Stats>,
    pricing: Res<Pricing>,
    active: Res<ActiveProduct>,
    appeal: Res<ProductAppeal>,
    cfg: Res<AiConfig>,
) {
    let att = (active.perf_index * cfg.0.product_weights.perf
        + appeal.0 * cfg.0.product_weights.appeal)
        .clamp(0.0, 1.0);
    let frac = (0.3 + 0.6 * att).clamp(0.0, 1.0);
    let sell_units = (stats.inventory_units as f64 * frac as f64) as u64;
    let revenue = pricing.asp_usd * Decimal::from(sell_units);
    let cost = pricing.unit_cost_usd * Decimal::from(sell_units);
    let profit = revenue - cost;
    stats.revenue_usd += revenue;
    stats.profit_usd += profit;
    stats.cogs_usd += cost;
    stats.last_sold_units = sell_units;
    stats.inventory_units = stats.inventory_units.saturating_sub(sell_units);
    info!(target: "sim.sales", sell_units, revenue = %stats.revenue_usd, profit = %stats.profit_usd, asp = %pricing.asp_usd, "Sales updated");
}

/// Finance system: placeholder for interests, cash flow, etc.
pub fn finance_system(stats: ResMut<Stats>) {
    // Contract billing handled in `finance_system_billing`
    info!(target: "sim.finance", profit = %stats.profit_usd, contract_costs_cents = stats.contract_costs_cents, "Finance tick");
}

/// Finance: charge foundry contracts monthly according to billing model.
pub fn finance_system_billing(
    mut stats: ResMut<Stats>,
    mut dom: ResMut<DomainWorld>,
    cap: Res<Capacity>,
    book: Res<CapacityBook>,
) {
    let date = dom.0.macro_state.date;
    let mut remaining_used_wafers = cap.wafers_per_month as i64;
    let mut total_cost_cents: i64 = 0;
    for c in &book.contracts {
        if !(date >= c.start && date <= c.end) {
            continue;
        }
        let committed = c.wafers_per_month as i64;
        let used_from_this = remaining_used_wafers.min(committed).max(0);
        remaining_used_wafers = (remaining_used_wafers - used_from_this).max(0);
        let min_bill = (c.take_or_pay_frac.clamp(0.0, 1.0) * (committed as f32)).ceil() as i64;
        let billed_wafers = used_from_this.max(min_bill);
        let price = if c.billing_cents_per_wafer > 0 {
            c.billing_cents_per_wafer
        } else {
            c.price_per_wafer_cents
        };
        let cost = billed_wafers.saturating_mul(price);
        total_cost_cents = total_cost_cents.saturating_add(cost);
    }
    stats.contract_costs_cents = stats.contract_costs_cents.saturating_add(total_cost_cents);
    // Deduct from cash of the first company for now
    if total_cost_cents > 0 {
        if let Some(c) = dom.0.companies.first_mut() {
            let dec = Decimal::from_i64(total_cost_cents).unwrap_or(Decimal::ZERO)
                / Decimal::from(100u64);
            c.cash_usd -= dec;
        }
    }
}

/// Advance tapeout queue and update product appeal when products are released.
pub fn tapeout_system(
    mut pipeline: ResMut<Pipeline>,
    mut appeal: ResMut<ProductAppeal>,
    mut active: ResMut<ActiveProduct>,
    mut pricing: ResMut<Pricing>,
    dom: Res<DomainWorld>,
    cfg_ai: Res<AiConfig>,
) {
    let date = dom.0.macro_state.date;
    let mut rest = Vec::with_capacity(pipeline.0.queue.len());
    let mut released_spec: Option<core::ProductSpec> = None;
    for req in pipeline.0.queue.drain(..) {
        if req.ready <= date {
            released_spec = Some(req.product);
        } else {
            rest.push(req);
        }
    }
    if let Some(spec) = released_spec {
        active.perf_index = spec.perf_index;
        // Recompute unit cost from node wafer cost, die area and yield
        let node = dom.0.tech_tree.iter().find(|n| n.id == spec.tech_node);
        if let Some(n) = node {
            let usable = cfg_ai.0.product_cost.usable_die_area_mm2.max(1.0);
            let units_per_wafer = ((usable / spec.die_area_mm2).floor() as i64).max(1);
            let overhead = cfg_ai.0.product_cost.yield_overhead_frac.clamp(0.0, 0.99);
            let eff_yield = (n.yield_baseline
                * Decimal::from_f32_retain(1.0 - overhead).unwrap_or(Decimal::ONE))
            .max(Decimal::new(1, 2));
            let denom = Decimal::from(units_per_wafer) * eff_yield;
            if denom > Decimal::ZERO {
                pricing.unit_cost_usd = n.wafer_cost_usd / denom;
            }
        }
        pipeline.0.released.push(spec);
        appeal.0 = (appeal.0 + 0.05).clamp(0.0, 0.5);
    }
    pipeline.0.queue = rest;
}

/// AI configuration resource loaded from defaults.
#[derive(Resource, Clone)]
pub struct AiConfig(pub ai::AiConfig);

fn stats_rd_boost(_stats: &Stats) -> f32 {
    0.0
}

/// AI strategy system: apply monthly tactics and quarterly plan signal.
pub fn ai_strategy_system(
    mut stats: ResMut<Stats>,
    dom: Res<DomainWorld>,
    cap: Res<Capacity>,
    mut pricing: ResMut<Pricing>,
    cfg: Res<AiConfig>,
    appeal: Res<ProductAppeal>,
) {
    // Compute demand/supply ratio for heuristics
    let seg = dom.0.segments.first();
    let (base_demand, elasticity) = if let Some(s) = seg {
        (s.base_demand_units, s.price_elasticity)
    } else {
        (100_000, -1.2)
    };
    let ref_price = pricing.asp_usd; // approximate
    let q_total = sim_econ::demand(base_demand, pricing.asp_usd, ref_price, elasticity)
        .unwrap_or(base_demand);
    let our_demand = ((q_total as f32) * stats.market_share).floor() as u64;
    let supply_units = cap
        .wafers_per_month
        .saturating_mul(50)
        .saturating_sub(cap.wafers_per_month.saturating_mul(50) / 20); // ~95% good dies
    let demand_supply_ratio = if supply_units == 0 {
        10.0
    } else {
        our_demand as f32 / (supply_units as f32)
    };

    // Tactics: price adjustments and R&D boost cuts
    let cm = ai::metrics_from_world(
        &dom.0,
        stats.market_share,
        stats.revenue_usd,
        stats.profit_usd,
    );
    let (price_df, rd_boost) = ai::decide_tactics(
        &cm,
        stats.last_share,
        demand_supply_ratio,
        pricing.unit_cost_usd,
        pricing.asp_usd,
        &cfg.0.tactics,
    );

    // Apply price change with floor
    let factor = rust_decimal::Decimal::from_f32_retain(1.0 + price_df).unwrap_or(Decimal::ONE);
    let mut new_price = pricing.asp_usd * factor;
    let min_price = pricing.unit_cost_usd
        * rust_decimal::Decimal::from_f32_retain(1.0 + cfg.0.tactics.min_margin_frac)
            .unwrap_or(Decimal::ONE);
    if new_price < min_price {
        new_price = min_price;
    }
    pricing.asp_usd = new_price;

    // Apply R&D boost as a small adjustment to progress directly (simplified)
    stats.rd_progress = (stats.rd_progress + rd_boost).clamp(0.0, 1.0);

    // Update market share drifting towards price-based target (simple proxy)
    let beta = cfg.0.planner.price_pref_beta;
    let comp_attr = cfg.0.planner.competitor_attractiveness.max(1e-3);
    let p = pricing.asp_usd.to_f32().unwrap_or(1.0).max(0.01);
    let r = ref_price.to_f32().unwrap_or(p).max(0.01);
    let a = (r / p).powf(beta) * (1.0 + appeal.0.clamp(0.0, 1.0));
    let target_share = (a / (a + comp_attr)).clamp(0.05, 0.95);
    stats.market_share += (target_share - stats.market_share) * 0.1;
    stats.market_share = stats.market_share.clamp(0.05, 0.95);

    // Quarterly planning moved to a separate system below
    // Update last_share tracker
    stats.last_share = stats.market_share;
    info!(target: "sim.ai", share = stats.market_share, asp = %pricing.asp_usd, rnd = stats.rd_progress, "AI strategy updated");
}

/// Quarterly planner integration: applies top decision to contracts/tapeouts.
pub fn ai_quarterly_planner_system(
    stats: Res<Stats>,
    mut dom: ResMut<DomainWorld>,
    mut pricing: ResMut<Pricing>,
    cfg: Res<AiConfig>,
    mut book: ResMut<CapacityBook>,
    mut pipeline: ResMut<Pipeline>,
) {
    if (stats.months_run + 1) % 3 != 0 {
        return;
    }
    // Compute approximate supply
    let supply_units = 0u64; // not needed for planner input's capacity
    let current = ai::CurrentKpis {
        asp_usd: pricing.asp_usd,
        unit_cost_usd: pricing.unit_cost_usd,
        capacity_units_per_month: supply_units,
        cash_usd: dom
            .0
            .companies
            .first()
            .map(|c| c.cash_usd)
            .unwrap_or(Decimal::ZERO),
        debt_usd: dom
            .0
            .companies
            .first()
            .map(|c| c.debt_usd)
            .unwrap_or(Decimal::ZERO),
        share: stats.market_share,
        rd_progress: stats.rd_progress,
    };
    let plan = ai::plan_horizon(&dom.0, &current, &cfg.0.weights, &cfg.0.planner);
    if let Some(first) = plan.decisions.first() {
        match first.action {
            ai::PlanAction::AdjustPriceFrac(df) => {
                let factor =
                    rust_decimal::Decimal::from_f32_retain(1.0 + df).unwrap_or(Decimal::ONE);
                let mut np = pricing.asp_usd * factor;
                let minp = pricing.unit_cost_usd
                    * rust_decimal::Decimal::from_f32_retain(1.0 + cfg.0.tactics.min_margin_frac)
                        .unwrap_or(Decimal::ONE);
                if np < minp {
                    np = minp;
                }
                pricing.asp_usd = np;
            }
            ai::PlanAction::AllocateRndBoost(_db) => {}
            ai::PlanAction::RequestCapacity(u) => {
                // Record a capacity contract to start after lead time
                let lead = cfg.0.planner.quarter_step as u8; // reuse quarter step as default lead time
                let start = dom.0.macro_state.date;
                // add months
                let (mut y, mut m) = (start.year(), start.month());
                let mut add = lead as u32;
                while add > 0 {
                    m += 1;
                    if m > 12 {
                        y += 1;
                        m = 1;
                    }
                    add -= 1;
                }
                let start_date =
                    chrono::NaiveDate::from_ymd_opt(y, m, start.day()).unwrap_or(start);
                let end_date =
                    chrono::NaiveDate::from_ymd_opt(y + 1, m, start.day()).unwrap_or(start_date);
                book.contracts.push(FoundryContract {
                    foundry_id: "FND-A".into(),
                    wafers_per_month: u as u32,
                    price_per_wafer_cents: 10_000,
                    take_or_pay_frac: 1.0,
                    billing_cents_per_wafer: 10_000,
                    billing_model: "take_or_pay",
                    lead_time_months: lead,
                    start: start_date,
                    end: end_date,
                });
            }
            ai::PlanAction::ScheduleTapeout { expedite } => {
                // Create a trivial product spec and push into pipeline
                let node_id = dom
                    .0
                    .tech_tree
                    .first()
                    .map(|n| n.id.clone())
                    .unwrap_or(core::TechNodeId("800nm".into()));
                let spec = core::ProductSpec {
                    kind: core::ProductKind::CPU,
                    tech_node: node_id.clone(),
                    microarch: core::MicroArch {
                        ipc_index: 1.0,
                        pipeline_depth: 10,
                        cache_l1_kb: 64,
                        cache_l2_mb: 1.0,
                        chiplet: false,
                    },
                    die_area_mm2: 100.0,
                    perf_index: 0.6,
                    tdp_w: 65.0,
                    bom_usd: 50.0,
                };
                let start = dom.0.macro_state.date;
                let mut ready = start;
                // Ready in 9 months baseline
                for _ in 0..9 {
                    let (mut y, mut m) = (ready.year(), ready.month());
                    m += 1;
                    if m > 12 {
                        y += 1;
                        m = 1;
                    }
                    ready = chrono::NaiveDate::from_ymd_opt(y, m, start.day()).unwrap_or(ready);
                }
                let mut expedite_cost = 0i64;
                if expedite {
                    // cut by 3 months with cost
                    for _ in 0..3 {
                        let (mut y, mut m) = (ready.year(), ready.month());
                        if m == 1 {
                            y -= 1;
                            m = 12;
                        } else {
                            m -= 1;
                        }
                        ready = chrono::NaiveDate::from_ymd_opt(y, m, start.day()).unwrap_or(ready);
                    }
                    expedite_cost = 100_000; // $1,000.00
                    if let Some(c) = dom.0.companies.first_mut() {
                        c.cash_usd -= Decimal::new(expedite_cost, 2);
                    }
                }
                let req = core::TapeoutRequest {
                    product: spec.clone(),
                    tech_node: node_id,
                    start,
                    ready,
                    expedite,
                    expedite_cost_cents: expedite_cost,
                };
                pipeline.0.queue.push(req);
            }
        }
    }
}

/// Create an ECS world with required resources from a domain world and config.
pub fn init_world(domain: core::World, config: core::SimConfig) -> World {
    let mut w = World::new();
    w.insert_resource(DomainWorld(domain));
    w.insert_resource(SimConfig(config));
    w.insert_resource(Stats::default());
    w.insert_resource(Capacity::default());
    w.insert_resource(CapacityBook::default());
    w.insert_resource(Pricing::default());
    w.insert_resource(ProductAppeal::default());
    w.insert_resource(ActiveProduct::default());
    w.insert_resource(Pipeline::default());
    w.insert_resource(RnDBudgetCents(0));
    // Load AI defaults from YAML via sim-ai
    let ai_cfg = ai::AiConfig::from_default_yaml().unwrap_or_default();
    w.insert_resource(AiConfig(ai_cfg));
    let rng = ChaCha8Rng::seed_from_u64(w.resource::<SimConfig>().0.rng_seed);
    w.insert_resource(RngResource(rng));
    w
}

/// Run monthly ticks and return a KPI snapshot and per-month telemetry.
pub fn run_months_with_telemetry(
    mut world: World,
    months: u32,
) -> (SimSnapshot, Vec<MonthlyTelemetry>) {
    let mut schedule = bevy_ecs::schedule::Schedule::default();
    use bevy_ecs::schedule::IntoSystemConfigs;
    schedule.add_systems(
        (
            r_and_d_system,
            foundry_capacity_system,
            production_system,
            tapeout_system,
            // capture month-level sales metrics
            (sales_system).after(production_system),
            (finance_system_billing, finance_system),
            ai_strategy_system,
            ai_quarterly_planner_system,
            advance_macro_date_system,
        )
            .chain(),
    );
    let mut telemetry = Vec::with_capacity(months as usize);
    for m in 0..months {
        schedule.run(&mut world);
        let pricing = world.resource::<Pricing>().clone();
        let mut stats = world.resource_mut::<Stats>();
        stats.months_run = stats.months_run.saturating_add(1);
        let sold_units = stats.last_sold_units;
        let asp = pricing.asp_usd;
        let unit_cost = pricing.unit_cost_usd;
        let revenue = asp * Decimal::from(sold_units);
        let margin = revenue - unit_cost * Decimal::from(sold_units);
        telemetry.push(MonthlyTelemetry {
            month_index: m + 1,
            output_units: stats.output_units,
            sold_units,
            asp_usd: asp,
            unit_cost_usd: unit_cost,
            margin_usd: margin,
            revenue_usd: revenue,
        });
    }
    world.remove_resource::<Capacity>();
    let snap = build_snapshot(&world);
    (snap, telemetry)
}

pub fn run_months(world: World, months: u32) -> SimSnapshot {
    let (snap, _t) = run_months_with_telemetry(world, months);
    snap
}

/// Run months in-place on an existing ECS world.
pub fn run_months_in_place(world: &mut World, months: u32) -> (SimSnapshot, Vec<MonthlyTelemetry>) {
    let mut schedule = bevy_ecs::schedule::Schedule::default();
    use bevy_ecs::schedule::IntoSystemConfigs;
    schedule.add_systems(
        (
            r_and_d_system,
            foundry_capacity_system,
            production_system,
            tapeout_system,
            (sales_system).after(production_system),
            (finance_system_billing, finance_system),
            ai_strategy_system,
            ai_quarterly_planner_system,
            advance_macro_date_system,
        )
            .chain(),
    );
    let mut telemetry = Vec::with_capacity(months as usize);
    for m in 0..months {
        schedule.run(world);
        let pricing = world.resource::<Pricing>().clone();
        let mut stats = world.resource_mut::<Stats>();
        stats.months_run = stats.months_run.saturating_add(1);
        let sold_units = stats.last_sold_units;
        let asp = pricing.asp_usd;
        let unit_cost = pricing.unit_cost_usd;
        let revenue = asp * Decimal::from(sold_units);
        let margin = revenue - unit_cost * Decimal::from(sold_units);
        telemetry.push(MonthlyTelemetry {
            month_index: m + 1,
            output_units: stats.output_units,
            sold_units,
            asp_usd: asp,
            unit_cost_usd: unit_cost,
            margin_usd: margin,
            revenue_usd: revenue,
        });
    }
    let _stats = world.resource::<Stats>().clone();
    let snap = build_snapshot(world);
    (snap, telemetry)
}

fn build_snapshot(world: &World) -> SimSnapshot {
    let stats = world.resource::<Stats>();
    let pricing = world.resource::<Pricing>();
    let dom = world.resource::<DomainWorld>();
    let cash = dom
        .0
        .companies
        .first()
        .map(|c| c.cash_usd)
        .unwrap_or(Decimal::ZERO);
    let cash_cents = persistence::decimal_to_cents_i64(cash).unwrap_or(0);
    let revenue_cents = persistence::decimal_to_cents_i64(stats.revenue_usd).unwrap_or(0);
    let cogs_cents = persistence::decimal_to_cents_i64(stats.cogs_usd).unwrap_or(0);
    let profit_cents = persistence::decimal_to_cents_i64(stats.profit_usd).unwrap_or(0);
    let asp_cents = persistence::decimal_to_cents_i64(pricing.asp_usd).unwrap_or(0);
    let unit_cost_cents = persistence::decimal_to_cents_i64(pricing.unit_cost_usd).unwrap_or(0);

    SimSnapshot {
        months_run: stats.months_run,
        cash_cents,
        revenue_cents,
        cogs_cents,
        profit_cents,
        contract_costs_cents: stats.contract_costs_cents,
        asp_cents,
        unit_cost_cents,
        market_share: stats.market_share,
        rd_progress: stats.rd_progress,
        output_units: stats.output_units,
        defect_units: stats.defect_units,
        inventory_units: stats.inventory_units,
    }
}

/// Rehydrate released products from persistence rows into runtime resources.
pub fn rehydrate_released_products(world: &mut World, rows: &[persistence::ReleasedRow]) {
    if rows.is_empty() {
        return;
    }
    // Parse specs first
    let mut specs: Vec<core::ProductSpec> = Vec::new();
    for r in rows {
        if let Ok(spec) = serde_json::from_str::<core::ProductSpec>(&r.product_json) {
            specs.push(spec);
        }
    }
    // Clone config and tech nodes snapshot for cost calc
    let ai_cfg = world.resource::<AiConfig>().0.clone();
    let tech_nodes = world.resource::<DomainWorld>().0.tech_tree.clone();
    drop(ai_cfg.clone()); // just to satisfy lint in case unused below

    // Extend pipeline and compute new count
    let last_spec = specs.last().cloned();
    let new_count: usize = {
        let mut pipe = world.resource_mut::<Pipeline>();
        let prev = pipe.0.released.len();
        pipe.0.released.extend(specs);
        prev + pipe.0.released.len() - prev
    };

    if let Some(last) = last_spec {
        // Active product
        {
            let mut active = world.resource_mut::<ActiveProduct>();
            active.perf_index = last.perf_index;
        }
        // Pricing unit cost
        if let Some(node) = tech_nodes.iter().find(|n| n.id == last.tech_node) {
            let mut pricing = world.resource_mut::<Pricing>();
            pricing.unit_cost_usd = compute_unit_cost(node, &last, &ai_cfg.product_cost);
        }
        // Appeal proportional to count
        {
            let mut appeal = world.resource_mut::<ProductAppeal>();
            appeal.0 = ((new_count as f32) * 0.05).clamp(0.0, 0.5);
        }
    }
}

/// Apply an ASP delta fraction requested by the player; returns new ASP.
pub fn apply_price_delta(world: &mut World, delta_frac: f32) -> Decimal {
    let cfg_min_margin = world.resource::<AiConfig>().0.tactics.min_margin_frac;
    let mut pricing = world.resource_mut::<Pricing>();
    let factor = rust_decimal::Decimal::from_f32_retain(1.0 + delta_frac).unwrap_or(Decimal::ONE);
    let mut np = pricing.asp_usd * factor;
    let minp = ai::min_price(pricing.unit_cost_usd, cfg_min_margin);
    if np < minp {
        np = minp;
    }
    pricing.asp_usd = np;
    np
}

/// Apply a delta to the player's monthly R&D budget (cents). Returns new budget.
pub fn apply_rd_delta(world: &mut World, delta_cents: i64) -> i64 {
    let mut b = world.resource_mut::<RnDBudgetCents>();
    let before = b.0;
    let after = before.saturating_add(delta_cents);
    b.0 = after.max(0);
    b.0
}

/// Create a capacity contract starting after planner lead time; returns a summary string.
pub fn apply_capacity_request(
    world: &mut World,
    wafers_per_month: u32,
    months: u16,
    billing_cents_per_wafer: Option<i64>,
    take_or_pay_frac: Option<f32>,
) -> String {
    let lead = world.resource::<AiConfig>().0.planner.quarter_step as u8;
    let start = world.resource::<DomainWorld>().0.macro_state.date;
    let mut book = world.resource_mut::<CapacityBook>();
    // compute start date by adding lead months
    let mut s = start;
    for _ in 0..lead {
        s = add_months(s, 1);
    }
    // end date after `months`
    let mut e = s;
    for _ in 0..months {
        e = add_months(e, 1);
    }
    let price = billing_cents_per_wafer.unwrap_or(10_000);
    let top = take_or_pay_frac.unwrap_or(1.0).clamp(0.0, 1.0);
    let c = FoundryContract {
        foundry_id: "FND-A".into(),
        wafers_per_month,
        price_per_wafer_cents: price,
        take_or_pay_frac: top,
        billing_cents_per_wafer: price,
        billing_model: "take_or_pay",
        lead_time_months: lead,
        start: s,
        end: e,
    };
    book.contracts.push(c);
    format!(
        "capacity: {} wpm, ${:.2}/wafer, top={:.0}% from {} to {}",
        wafers_per_month,
        (rust_decimal::Decimal::from(price) / Decimal::from(100u64)),
        (top * 100.0),
        s,
        e
    )
}

/// Schedule a tapeout; optionally expedite and charge cost; returns ready date.
pub fn apply_tapeout_request(
    world: &mut World,
    perf_index: f32,
    die_area_mm2: f32,
    tech_node: String,
    expedite: bool,
) -> chrono::NaiveDate {
    let dom_date = world.resource::<DomainWorld>().0.macro_state.date;
    let node_id = core::TechNodeId(tech_node);
    let spec = core::ProductSpec {
        kind: core::ProductKind::CPU,
        tech_node: node_id.clone(),
        microarch: core::MicroArch {
            ipc_index: 1.0,
            pipeline_depth: 10,
            cache_l1_kb: 64,
            cache_l2_mb: 1.0,
            chiplet: false,
        },
        die_area_mm2,
        perf_index,
        tdp_w: 65.0,
        bom_usd: 50.0,
    };
    let mut ready = dom_date;
    // baseline 9 months
    for _ in 0..9 {
        ready = add_months(ready, 1);
    }
    let mut expedite_cost = 0i64;
    if expedite {
        // cut 3 months
        for _ in 0..3 {
            // subtract one month by adding 11 months then normalizing year would be complex; easier: step back month-wise
            // We'll recompute by stepping back via chrono logic: find previous month same day or clamp
            let y = ready.year();
            let m = ready.month();
            let d = ready.day();
            let (y2, m2) = if m == 1 {
                (y - 1, 12)
            } else {
                (y, m as i32 - 1)
            };
            let mut day = d;
            let mut cand = chrono::NaiveDate::from_ymd_opt(y2, m2 as u32, day);
            while cand.is_none() && day > 28 {
                day -= 1;
                cand = chrono::NaiveDate::from_ymd_opt(y2, m2 as u32, day);
            }
            ready =
                cand.unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(y2, m2 as u32, 1).unwrap());
        }
        expedite_cost = 100_000; // $1,000.00
                                 // charge cash
        let mut dom = world.resource_mut::<DomainWorld>();
        if let Some(c) = dom.0.companies.first_mut() {
            c.cash_usd -= Decimal::new(expedite_cost, 2);
        }
    }
    // enqueue
    let mut pipe = world.resource_mut::<Pipeline>();
    pipe.0.queue.push(core::TapeoutRequest {
        product: spec,
        tech_node: node_id,
        start: dom_date,
        ready,
        expedite,
        expedite_cost_cents: expedite_cost,
    });
    ready
}

/// Advance macro date by one calendar month per tick.
pub fn advance_macro_date_system(mut dom: ResMut<DomainWorld>) {
    let cur = dom.0.macro_state.date;
    dom.0.macro_state.date = add_months(cur, 1);
}

/// Add `n` months to a date, clamping the day to the end of month when needed.
fn add_months(mut d: NaiveDate, mut n: u32) -> NaiveDate {
    if n == 0 {
        return d;
    }
    let orig_day = d.day();
    let mut y = d.year();
    let mut m = d.month();
    while n > 0 {
        m += 1;
        if m > 12 {
            m = 1;
            y += 1;
        }
        // try same day; if invalid, step back until valid
        let mut day = orig_day;
        let cand = NaiveDate::from_ymd_opt(y, m, day);
        d = if let Some(ok) = cand {
            ok
        } else {
            // find last valid day of month
            let mut found: Option<NaiveDate> = None;
            while day > 28 {
                day -= 1;
                if let Some(ok) = NaiveDate::from_ymd_opt(y, m, day) {
                    found = Some(ok);
                    break;
                }
            }
            // Fallback to day 1 if somehow didn't find one
            found.unwrap_or_else(|| NaiveDate::from_ymd_opt(y, m, 1).unwrap())
        };
        n -= 1;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use tokio::runtime::Runtime;

    #[test]
    fn world_creates_and_ticks() {
        let dom = core::World {
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![],
            companies: vec![],
            segments: vec![],
        };
        let cfg = core::SimConfig {
            tick_days: 30,
            rng_seed: 42,
        };
        let w = init_world(dom, cfg);
        let snap = run_months(w, 3);
        assert_eq!(snap.months_run, 3);
        assert!(snap.rd_progress >= 0.0);
        assert!(snap.output_units > 0u64);
        assert!(snap.revenue_cents >= 0);
    }

    #[test]
    fn calendar_advances_monthly_and_rolls_year() {
        let dom = core::World {
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1997, 12, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: Decimal::new(1_000_000, 0),
                debt_usd: Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1000,
                price_elasticity: -1.2,
            }],
        };
        let cfg = core::SimConfig {
            tick_days: 30,
            rng_seed: 1,
        };
        let mut w = init_world(dom, cfg);
        let _ = run_months_in_place(&mut w, 2);
        let date = w.resource::<DomainWorld>().0.macro_state.date;
        assert_eq!(date, chrono::NaiveDate::from_ymd_opt(1998, 2, 1).unwrap());
    }

    #[test]
    fn ai_tactics_lower_price_on_share_drop_with_floor() {
        let dom = core::World {
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: Decimal::new(1_000_000, 0),
                debt_usd: Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.2,
            }],
        };
        let cfg = core::SimConfig {
            tick_days: 30,
            rng_seed: 42,
        };
        let mut w = init_world(dom, cfg);
        {
            let mut stats = w.resource_mut::<Stats>();
            stats.market_share = 0.30;
            stats.last_share = 0.50; // drop 0.20
        }
        {
            let mut pricing = w.resource_mut::<Pricing>();
            pricing.asp_usd = Decimal::new(220, 0);
            pricing.unit_cost_usd = Decimal::new(200, 0);
        }
        // Run just the AI system once
        let mut schedule = bevy_ecs::schedule::Schedule::default();
        schedule.add_systems(ai_strategy_system);
        schedule.run(&mut w);
        let pricing = w.resource::<Pricing>();
        // Expected price lower but not below 5% margin floor: min price = 210
        assert!(pricing.asp_usd >= Decimal::new(210, 0));
        assert!(pricing.asp_usd <= Decimal::new(220, 0));
    }

    #[test]
    fn ai_tactics_raise_price_on_shortage() {
        let dom = core::World {
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: Decimal::new(1_000_000, 0),
                debt_usd: Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.2,
            }],
        };
        let cfg = core::SimConfig {
            tick_days: 30,
            rng_seed: 42,
        };
        let mut w = init_world(dom, cfg);
        {
            let mut stats = w.resource_mut::<Stats>();
            stats.market_share = 0.50;
            stats.last_share = 0.50;
        }
        {
            // Severe shortage
            let mut cap = w.resource_mut::<Capacity>();
            cap.wafers_per_month = 100; // tiny supply
            let mut pricing = w.resource_mut::<Pricing>();
            pricing.asp_usd = Decimal::new(300, 0);
            pricing.unit_cost_usd = Decimal::new(200, 0);
        }
        let mut schedule = bevy_ecs::schedule::Schedule::default();
        schedule.add_systems(ai_strategy_system);
        schedule.run(&mut w);
        let pricing = w.resource::<Pricing>();
        assert!(pricing.asp_usd > Decimal::new(300, 0));
    }

    #[test]
    fn stronger_product_sells_more() {
        let dom = core::World {
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![core::TechNode {
                id: core::TechNodeId("N90".into()),
                year_available: 1990,
                density_mtr_per_mm2: Decimal::new(1, 0),
                freq_ghz_baseline: Decimal::new(1, 0),
                leakage_index: Decimal::new(1, 0),
                yield_baseline: Decimal::new(9, 1),
                wafer_cost_usd: Decimal::new(1000, 0),
                mask_set_cost_usd: Decimal::new(5000, 0),
                dependencies: vec![],
            }],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: Decimal::new(1_000_000, 0),
                debt_usd: Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.2,
            }],
        };
        let cfg = core::SimConfig {
            tick_days: 30,
            rng_seed: 42,
        };
        // World A: weaker product
        let mut wa = init_world(dom.clone(), cfg.clone());
        {
            let mut ap = wa.resource_mut::<ActiveProduct>();
            ap.perf_index = 0.2;
            let mut stats = wa.resource_mut::<Stats>();
            stats.inventory_units = 100_000;
        }
        let mut sched = bevy_ecs::schedule::Schedule::default();
        sched.add_systems(sales_system);
        sched.run(&mut wa);
        let sold_a = wa.resource::<Stats>().last_sold_units;
        // World B: stronger product
        let mut wb = init_world(dom, cfg);
        {
            let mut ap = wb.resource_mut::<ActiveProduct>();
            ap.perf_index = 0.9;
            let mut stats = wb.resource_mut::<Stats>();
            stats.inventory_units = 100_000;
        }
        let mut sched2 = bevy_ecs::schedule::Schedule::default();
        sched2.add_systems(sales_system);
        sched2.run(&mut wb);
        let sold_b = wb.resource::<Stats>().last_sold_units;
        assert!(sold_b > sold_a);
    }

    #[test]
    fn unit_cost_monotonicity() {
        let node = core::TechNode {
            id: core::TechNodeId("N90".into()),
            year_available: 1990,
            density_mtr_per_mm2: Decimal::new(1, 0),
            freq_ghz_baseline: Decimal::new(1, 0),
            leakage_index: Decimal::new(1, 0),
            yield_baseline: Decimal::new(9, 1),
            wafer_cost_usd: Decimal::new(1000, 0),
            mask_set_cost_usd: Decimal::new(5000, 0),
            dependencies: vec![],
        };
        let cfg = ai::ProductCostCfg {
            usable_die_area_mm2: 6200.0,
            yield_overhead_frac: 0.05,
        };
        let spec_small = core::ProductSpec {
            kind: core::ProductKind::CPU,
            tech_node: core::TechNodeId("N90".into()),
            microarch: core::MicroArch {
                ipc_index: 1.0,
                pipeline_depth: 10,
                cache_l1_kb: 64,
                cache_l2_mb: 1.0,
                chiplet: false,
            },
            die_area_mm2: 100.0,
            perf_index: 0.5,
            tdp_w: 65.0,
            bom_usd: 50.0,
        };
        let mut spec_large = spec_small.clone();
        spec_large.die_area_mm2 = 200.0;
        let cost_small = compute_unit_cost(&node, &spec_small, &cfg);
        let cost_large = compute_unit_cost(&node, &spec_large, &cfg);
        assert!(cost_large > cost_small);
        // Yield higher lowers cost
        let mut node2 = node.clone();
        node2.yield_baseline = Decimal::new(95, 2); // 0.95
        let cost_high_yield = compute_unit_cost(&node2, &spec_small, &cfg);
        assert!(cost_high_yield < cost_small);
    }

    #[test]
    fn deterministic_kpis_with_same_seed() {
        let dom = core::World {
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: Decimal::new(1_000_000, 0),
                debt_usd: Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.2,
            }],
        };
        let cfg = core::SimConfig {
            tick_days: 30,
            rng_seed: 123,
        };
        let snap1 = run_months(init_world(dom.clone(), cfg.clone()), 36);
        let snap2 = run_months(init_world(dom.clone(), cfg.clone()), 36);
        assert_eq!(snap1.months_run, snap2.months_run);
        assert_eq!(snap1.revenue_cents, snap2.revenue_cents);
        assert_eq!(snap1.profit_cents, snap2.profit_cents);
        assert!((snap1.market_share - snap2.market_share).abs() < f32::EPSILON);
    }

    #[test]
    fn rehydrate_from_db_applies_contracts_and_tapeout() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async move {
            let pool = persistence::init_db("sqlite::memory:").await.unwrap();
            let save_id = persistence::create_save(&pool, "s", None).await.unwrap();
            // Insert a contract billed this month
            let c = persistence::ContractRow {
                foundry_id: "F1".into(),
                wafers_per_month: 3000,
                price_per_wafer_cents: 1000,
                take_or_pay_frac: 1.0,
                billing_cents_per_wafer: 1000,
                billing_model: "take_or_pay".into(),
                lead_time_months: 0,
                start: "1990-01-01".into(),
                end: "1990-12-01".into(),
            };
            let _ = persistence::insert_contract(&pool, save_id, &c)
                .await
                .unwrap();
            // Tapeout ready next month
            let spec = core::ProductSpec {
                kind: core::ProductKind::CPU,
                tech_node: core::TechNodeId("N90".into()),
                microarch: core::MicroArch {
                    ipc_index: 1.0,
                    pipeline_depth: 10,
                    cache_l1_kb: 64,
                    cache_l2_mb: 1.0,
                    chiplet: false,
                },
                die_area_mm2: 100.0,
                perf_index: 0.6,
                tdp_w: 65.0,
                bom_usd: 50.0,
            };
            let t = persistence::TapeoutRow {
                product_json: serde_json::to_string(&spec).unwrap(),
                tech_node: "N90".into(),
                start: "1990-01-01".into(),
                ready: "1990-01-01".into(),
                expedite: 0,
                expedite_cost_cents: 0,
            };
            let _ = persistence::insert_tapeout_request(&pool, save_id, &t)
                .await
                .unwrap();

            // Load rows and hydrate resources
            let conrows = persistence::list_contracts(&pool, save_id).await.unwrap();
            let taprows = persistence::list_tapeout_requests(&pool, save_id)
                .await
                .unwrap();

            let dom = core::World {
                macro_state: core::MacroState {
                    date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                    inflation_annual: 0.02,
                    interest_rate: 0.05,
                    fx_usd_index: 100.0,
                },
                tech_tree: vec![core::TechNode {
                    id: core::TechNodeId("N90".into()),
                    year_available: 1990,
                    density_mtr_per_mm2: Decimal::new(1, 0),
                    freq_ghz_baseline: Decimal::new(1, 0),
                    leakage_index: Decimal::new(1, 0),
                    yield_baseline: Decimal::new(9, 1),
                    wafer_cost_usd: Decimal::new(1000, 0),
                    mask_set_cost_usd: Decimal::new(5000, 0),
                    dependencies: vec![],
                }],
                companies: vec![core::Company {
                    name: "A".into(),
                    cash_usd: Decimal::new(1_000_000, 0),
                    debt_usd: Decimal::ZERO,
                    ip_portfolio: vec![],
                }],
                segments: vec![core::MarketSegment {
                    name: "Seg".into(),
                    base_demand_units: 1_000_000,
                    price_elasticity: -1.2,
                }],
            };
            let cfg = core::SimConfig {
                tick_days: 30,
                rng_seed: 1,
            };
            let mut w = init_world(dom, cfg);
            // Map into runtime resources
            {
                let mut book = w.resource_mut::<CapacityBook>();
                for r in conrows {
                    let start = chrono::NaiveDate::parse_from_str(&r.start, "%Y-%m-%d").unwrap();
                    let end = chrono::NaiveDate::parse_from_str(&r.end, "%Y-%m-%d").unwrap();
                    book.contracts.push(FoundryContract {
                        foundry_id: r.foundry_id,
                        wafers_per_month: r.wafers_per_month as u32,
                        price_per_wafer_cents: r.price_per_wafer_cents,
                        take_or_pay_frac: r.take_or_pay_frac,
                        billing_cents_per_wafer: r.billing_cents_per_wafer,
                        billing_model: Box::leak(r.billing_model.into_boxed_str()),
                        lead_time_months: r.lead_time_months as u8,
                        start,
                        end,
                    });
                }
                let mut pipe = w.resource_mut::<Pipeline>();
                for t in taprows {
                    let start = chrono::NaiveDate::parse_from_str(&t.start, "%Y-%m-%d").unwrap();
                    let ready = chrono::NaiveDate::parse_from_str(&t.ready, "%Y-%m-%d").unwrap();
                    let spec: core::ProductSpec = serde_json::from_str(&t.product_json).unwrap();
                    pipe.0.queue.push(core::TapeoutRequest {
                        product: spec,
                        tech_node: core::TechNodeId(t.tech_node),
                        start,
                        ready,
                        expedite: t.expedite != 0,
                        expedite_cost_cents: t.expedite_cost_cents,
                    });
                }
            }
            // Tick month: contract billed and tapeout released (appeal rises)
            let (snap1, _t) = run_months_in_place(&mut w, 1);
            assert!(snap1.contract_costs_cents >= 3_000_000);
            assert!(w.resource::<ProductAppeal>().0 > 0.0);
        });
    }

    #[test]
    fn multi_company_shares_not_degenerate() {
        let dom = core::World {
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![],
            companies: vec![
                core::Company {
                    name: "A".into(),
                    cash_usd: Decimal::new(1_000_000, 0),
                    debt_usd: Decimal::ZERO,
                    ip_portfolio: vec![],
                },
                core::Company {
                    name: "B".into(),
                    cash_usd: Decimal::new(1_000_000, 0),
                    debt_usd: Decimal::ZERO,
                    ip_portfolio: vec![],
                },
                core::Company {
                    name: "C".into(),
                    cash_usd: Decimal::new(1_000_000, 0),
                    debt_usd: Decimal::ZERO,
                    ip_portfolio: vec![],
                },
                core::Company {
                    name: "D".into(),
                    cash_usd: Decimal::new(1_000_000, 0),
                    debt_usd: Decimal::ZERO,
                    ip_portfolio: vec![],
                },
                core::Company {
                    name: "E".into(),
                    cash_usd: Decimal::new(1_000_000, 0),
                    debt_usd: Decimal::ZERO,
                    ip_portfolio: vec![],
                },
            ],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.2,
            }],
        };
        let cfg = core::SimConfig {
            tick_days: 30,
            rng_seed: 999,
        };
        let snap = run_months(init_world(dom, cfg), 48);
        assert!(snap.market_share > 0.05 && snap.market_share < 0.95);
    }

    #[test]
    fn rehydrate_released_products_sets_active_and_sales() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async move {
            let pool = persistence::init_db("sqlite::memory:").await.unwrap();
            let save_id = persistence::create_save(&pool, "s", None).await.unwrap();
            // Prepare one released product
            let spec = core::ProductSpec {
                kind: core::ProductKind::CPU,
                tech_node: core::TechNodeId("N90".into()),
                microarch: core::MicroArch {
                    ipc_index: 1.0,
                    pipeline_depth: 10,
                    cache_l1_kb: 64,
                    cache_l2_mb: 1.0,
                    chiplet: false,
                },
                die_area_mm2: 100.0,
                perf_index: 0.75,
                tdp_w: 65.0,
                bom_usd: 50.0,
            };
            let row = persistence::ReleasedRow {
                product_json: serde_json::to_string(&spec).unwrap(),
                released_at: "1990-01-01".into(),
            };
            let _ = persistence::insert_released_product(&pool, save_id, &row)
                .await
                .unwrap();

            let rows = persistence::list_released_products(&pool, save_id)
                .await
                .unwrap();

            // Domain world with matching tech node
            let dom = core::World {
                macro_state: core::MacroState {
                    date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                    inflation_annual: 0.02,
                    interest_rate: 0.05,
                    fx_usd_index: 100.0,
                },
                tech_tree: vec![core::TechNode {
                    id: core::TechNodeId("N90".into()),
                    year_available: 1989,
                    density_mtr_per_mm2: Decimal::new(1, 0),
                    freq_ghz_baseline: Decimal::new(1, 0),
                    leakage_index: Decimal::new(1, 0),
                    yield_baseline: Decimal::new(9, 1),
                    wafer_cost_usd: Decimal::new(1000, 0),
                    mask_set_cost_usd: Decimal::new(5000, 0),
                    dependencies: vec![],
                }],
                companies: vec![core::Company {
                    name: "A".into(),
                    cash_usd: Decimal::new(1_000_000, 0),
                    debt_usd: Decimal::ZERO,
                    ip_portfolio: vec![],
                }],
                segments: vec![core::MarketSegment {
                    name: "Seg".into(),
                    base_demand_units: 1_000_000,
                    price_elasticity: -1.2,
                }],
            };
            let cfg = core::SimConfig {
                tick_days: 30,
                rng_seed: 7,
            };
            let mut w = init_world(dom, cfg);
            // Rehydrate and verify
            rehydrate_released_products(&mut w, &rows);
            assert!((w.resource::<ActiveProduct>().perf_index - 0.75).abs() < f32::EPSILON);
            assert!(w.resource::<ProductAppeal>().0 > 0.0);
            let unit_cost = w.resource::<Pricing>().unit_cost_usd;
            assert!(unit_cost > Decimal::ZERO);
            // Run a month and ensure some sales/revenue
            let (snap, _t) = run_months_in_place(&mut w, 1);
            assert!(snap.revenue_cents > 0);
            assert!(w.resource::<Stats>().last_sold_units > 0);
        });
    }

    #[test]
    fn capacity_contract_increases_after_lead_time() {
        use chrono::Datelike;
        let dom = core::World {
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: Decimal::new(1_000_000, 0),
                debt_usd: Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.2,
            }],
        };
        let cfg = core::SimConfig {
            tick_days: 30,
            rng_seed: 1,
        };
        let mut w = init_world(dom.clone(), cfg);
        // Initial capacity via schedule
        let mut sched = bevy_ecs::schedule::Schedule::default();
        sched.add_systems(foundry_capacity_system);
        sched.run(&mut w);
        let base = w.resource::<Capacity>().wafers_per_month;
        // Add a contract with lead time 2 months (start at +2 months)
        let start = dom.macro_state.date;
        let (mut y, mut m) = (start.year(), start.month());
        m += 2;
        if m > 12 {
            y += 1;
            m -= 12;
        }
        let start_plus_2 = chrono::NaiveDate::from_ymd_opt(y, m, start.day()).unwrap();
        {
            let mut book = w.resource_mut::<CapacityBook>();
            book.contracts.push(FoundryContract {
                foundry_id: "F1".into(),
                wafers_per_month: 500,
                price_per_wafer_cents: 10_000,
                take_or_pay_frac: 1.0,
                billing_cents_per_wafer: 10_000,
                billing_model: "take_or_pay",
                lead_time_months: 2,
                start: start_plus_2,
                end: chrono::NaiveDate::from_ymd_opt(y + 1, m, start.day()).unwrap_or(start_plus_2),
            });
        }
        // Capacity should remain base until date reaches contract.start
        sched.run(&mut w);
        assert_eq!(w.resource::<Capacity>().wafers_per_month, base);
        // Advance to the start_plus_2 month
        {
            let mut dw = w.resource_mut::<DomainWorld>();
            dw.0.macro_state.date = start_plus_2;
        }
        sched.run(&mut w);
        // After passing start date, capacity should increase
        assert!(w.resource::<Capacity>().wafers_per_month > base);
    }

    #[test]
    fn take_or_pay_bills_even_when_underused() {
        use chrono::Datelike;
        let dom = core::World {
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: Decimal::new(1_000_000, 0),
                debt_usd: Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![],
        };
        let cfg = core::SimConfig {
            tick_days: 30,
            rng_seed: 1,
        };
        let mut w = init_world(dom.clone(), cfg);
        // Add an active contract for this month
        let start = dom.macro_state.date;
        let end =
            chrono::NaiveDate::from_ymd_opt(start.year(), start.month(), start.day()).unwrap();
        {
            let mut book = w.resource_mut::<CapacityBook>();
            book.contracts.push(FoundryContract {
                foundry_id: "F1".into(),
                wafers_per_month: 3000,
                price_per_wafer_cents: 1000,
                take_or_pay_frac: 1.0,
                billing_cents_per_wafer: 1000,
                billing_model: "take_or_pay",
                lead_time_months: 0,
                start,
                end,
            });
        }
        // Force underuse: zero out used wafers this month
        {
            let mut cap = w.resource_mut::<Capacity>();
            cap.wafers_per_month = 0;
        }
        // Run finance billing only
        let mut sched = bevy_ecs::schedule::Schedule::default();
        sched.add_systems(finance_system_billing);
        sched.run(&mut w);
        let stats = w.resource::<Stats>();
        // Expect billed: 3000 * 1000 cents
        assert_eq!(stats.contract_costs_cents, 3_000_000);
        // Cash decreased by $30,000.00
        let cash = w
            .resource::<DomainWorld>()
            .0
            .companies
            .first()
            .unwrap()
            .cash_usd;
        assert!(cash < Decimal::new(1_000_000, 0));
    }

    #[test]
    fn take_or_pay_bills_full_even_when_partially_used() {
        use chrono::Datelike;
        let dom = core::World {
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: Decimal::new(1_000_000, 0),
                debt_usd: Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![],
        };
        let cfg = core::SimConfig {
            tick_days: 30,
            rng_seed: 1,
        };
        let mut w = init_world(dom.clone(), cfg);
        let start = dom.macro_state.date;
        let end =
            chrono::NaiveDate::from_ymd_opt(start.year(), start.month(), start.day()).unwrap();
        {
            let mut book = w.resource_mut::<CapacityBook>();
            book.contracts.push(FoundryContract {
                foundry_id: "F1".into(),
                wafers_per_month: 3000,
                price_per_wafer_cents: 1000,
                take_or_pay_frac: 1.0,
                billing_cents_per_wafer: 1000,
                billing_model: "take_or_pay",
                lead_time_months: 0,
                start,
                end,
            });
        }
        // Partial usage: 1000 wafers used
        {
            let mut cap = w.resource_mut::<Capacity>();
            cap.wafers_per_month = 1000;
        }
        let mut sched = bevy_ecs::schedule::Schedule::default();
        sched.add_systems(finance_system_billing);
        sched.run(&mut w);
        let stats = w.resource::<Stats>();
        // Still billed 3000
        assert_eq!(stats.contract_costs_cents, 3_000_000);
    }

    #[test]
    fn expedite_tapeout_reduces_ready_and_spends_cash() {
        let dom = core::World {
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![core::TechNode {
                id: core::TechNodeId("N90".into()),
                year_available: 1990,
                density_mtr_per_mm2: Decimal::new(1, 0),
                freq_ghz_baseline: Decimal::new(1, 0),
                leakage_index: Decimal::new(1, 0),
                yield_baseline: Decimal::new(9, 1),
                wafer_cost_usd: Decimal::new(1000, 0),
                mask_set_cost_usd: Decimal::new(5000, 0),
                dependencies: vec![],
            }],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: Decimal::new(10_000_00, 2),
                debt_usd: Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.2,
            }],
        };
        let cfg = core::SimConfig {
            tick_days: 30,
            rng_seed: 7,
        };
        let mut w = init_world(dom.clone(), cfg);
        let start = dom.macro_state.date;
        // Manually create an expedited tapeout
        {
            let mut pipe = w.resource_mut::<Pipeline>();
            // Ready baseline after 9 months, expedited by 3 months
            let mut ready = start;
            for _ in 0..6 {
                let (mut y, mut m) = (ready.year(), ready.month());
                m += 1;
                if m > 12 {
                    y += 1;
                    m = 1;
                }
                ready = chrono::NaiveDate::from_ymd_opt(y, m, start.day()).unwrap_or(ready);
            }
            let spec = core::ProductSpec {
                kind: core::ProductKind::CPU,
                tech_node: core::TechNodeId("N90".into()),
                microarch: core::MicroArch {
                    ipc_index: 1.0,
                    pipeline_depth: 10,
                    cache_l1_kb: 64,
                    cache_l2_mb: 1.0,
                    chiplet: false,
                },
                die_area_mm2: 100.0,
                perf_index: 0.8,
                tdp_w: 65.0,
                bom_usd: 50.0,
            };
            pipe.0.queue.push(core::TapeoutRequest {
                product: spec,
                tech_node: core::TechNodeId("N90".into()),
                start,
                ready,
                expedite: true,
                expedite_cost_cents: 100_000,
            });
        }
        // Spend expedite cost
        {
            let mut dw = w.resource_mut::<DomainWorld>();
            if let Some(c) = dw.0.companies.first_mut() {
                c.cash_usd -= Decimal::new(100_000, 2);
            }
        }
        // Advance date to ready
        {
            let mut dw = w.resource_mut::<DomainWorld>();
            let (mut y, mut m) = (start.year(), start.month());
            for _ in 0..6 {
                m += 1;
                if m > 12 {
                    y += 1;
                    m = 1;
                }
            }
            dw.0.macro_state.date = chrono::NaiveDate::from_ymd_opt(y, m, start.day()).unwrap();
        }
        // Run tapeout system
        let mut sched = bevy_ecs::schedule::Schedule::default();
        sched.add_systems(tapeout_system);
        sched.run(&mut w);
        // Released should be non-empty; appeal increased; cash decreased
        assert!(!w.resource::<Pipeline>().0.released.is_empty());
        assert!(w.resource::<ProductAppeal>().0 > 0.0);
        let cash = w
            .resource::<DomainWorld>()
            .0
            .companies
            .first()
            .unwrap()
            .cash_usd;
        assert!(cash < Decimal::new(10_000_00, 2));
    }
}
/// Compute unit cost based on node, spec, and AI product-cost config.
pub fn compute_unit_cost(
    node: &core::TechNode,
    spec: &core::ProductSpec,
    cfg: &ai::ProductCostCfg,
) -> Decimal {
    let usable = cfg.usable_die_area_mm2.max(1.0);
    let units_per_wafer = ((usable / spec.die_area_mm2).floor() as i64).max(1);
    let overhead = cfg.yield_overhead_frac.clamp(0.0, 0.99);
    let eff_yield = (node.yield_baseline
        * Decimal::from_f32_retain(1.0 - overhead).unwrap_or(Decimal::ONE))
    .max(Decimal::new(1, 2));
    let denom = Decimal::from(units_per_wafer) * eff_yield;
    if denom > Decimal::ZERO {
        node.wafer_cost_usd / denom
    } else {
        node.wafer_cost_usd
    }
}
