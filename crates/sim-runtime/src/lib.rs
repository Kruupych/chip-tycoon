#![deny(warnings)]

//! ECS runtime for the simulation.
//!
//! Exposes a simple monthly tick runner with deterministic, stubbed systems.

use bevy_ecs::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rust_decimal::Decimal;
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
    pub market_share: f32,
    pub rd_progress: f32,
    pub output_units: u64,
    pub defect_units: u64,
    pub inventory_units: u64,
}

/// Snapshot of aggregated KPIs after running the simulation.
#[derive(Clone, Debug)]
pub struct SimSnapshot {
    pub months_run: u32,
    pub revenue_usd: Decimal,
    pub profit_usd: Decimal,
    pub market_share: f32,
    pub rd_progress: f32,
    pub output_units: u64,
    pub defect_units: u64,
    pub inventory_units: u64,
}

impl From<Stats> for SimSnapshot {
    fn from(s: Stats) -> Self {
        SimSnapshot {
            months_run: s.months_run,
            revenue_usd: s.revenue_usd,
            profit_usd: s.profit_usd,
            market_share: s.market_share,
            rd_progress: s.rd_progress,
            output_units: s.output_units,
            defect_units: s.defect_units,
            inventory_units: s.inventory_units,
        }
    }
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

/// R&D system: increases R&D progress deterministically.
pub fn r_and_d_system(mut stats: ResMut<Stats>) {
    let inc = 0.01f32; // 1% per month baseline
    stats.rd_progress = (stats.rd_progress + inc).clamp(0.0, 1.0);
    info!(target: "sim.rnd", rd_progress = stats.rd_progress, "R&D progress updated");
}

/// Foundry capacity: placeholder system to influence production.
#[derive(Resource, Default)]
pub struct Capacity {
    pub wafers_per_month: u64,
}

/// Global RNG resource seeded from `SimConfig` for deterministic noise.
#[derive(Resource)]
pub struct RngResource(pub ChaCha8Rng);

pub fn foundry_capacity_system(mut cap: ResMut<Capacity>, dom: Res<DomainWorld>) {
    // Deterministic dummy capacity: base on number of tech nodes and companies.
    let base = 1000u64;
    let factor = (dom.0.tech_tree.len() as u64 + dom.0.companies.len() as u64).max(1);
    cap.wafers_per_month = base * factor;
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

/// Sales system: sells some inventory at a fixed ASP and cost.
pub fn sales_system(mut stats: ResMut<Stats>) {
    let sell_units = (stats.inventory_units as f64 * 0.5) as u64; // sell 50%
    let asp = Decimal::new(300, 0); // $300
    let unit_cost = Decimal::new(200, 0); // $200
    let revenue = asp * Decimal::from(sell_units);
    let cost = unit_cost * Decimal::from(sell_units);
    let profit = revenue - cost;
    stats.revenue_usd += revenue;
    stats.profit_usd += profit;
    stats.inventory_units = stats.inventory_units.saturating_sub(sell_units);
    info!(target: "sim.sales", sell_units, revenue = %stats.revenue_usd, profit = %stats.profit_usd, "Sales updated");
}

/// Finance system: placeholder for interests, cash flow, etc.
pub fn finance_system(stats: ResMut<Stats>) {
    // Simple carry-over, ensure profit is not NaN (never is for Decimal).
    info!(target: "sim.finance", profit = %stats.profit_usd, "Finance tick");
}

/// AI strategy system: adjusts market share towards a baseline.
pub fn ai_strategy_system(mut stats: ResMut<Stats>) {
    let target_share = 0.5f32; // baseline 50%
    stats.market_share += (target_share - stats.market_share) * 0.1;
    info!(target: "sim.ai", share = stats.market_share, "AI strategy updated");
}

/// Create an ECS world with required resources from a domain world and config.
pub fn init_world(domain: core::World, config: core::SimConfig) -> World {
    let mut w = World::new();
    w.insert_resource(DomainWorld(domain));
    w.insert_resource(SimConfig(config));
    w.insert_resource(Stats::default());
    w.insert_resource(Capacity::default());
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
            // capture month-level sales metrics
            (sales_system).after(production_system),
            finance_system,
            ai_strategy_system,
        )
            .chain(),
    );
    let mut telemetry = Vec::with_capacity(months as usize);
    for m in 0..months {
        schedule.run(&mut world);
        let mut stats = world.resource_mut::<Stats>();
        stats.months_run = stats.months_run.saturating_add(1);
        let sold_units = (stats.inventory_units as f64 * 0.5) as u64; // mirrors sales_system
        let asp = Decimal::new(300, 0);
        let unit_cost = Decimal::new(200, 0);
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
    let stats = world.remove_resource::<Stats>().unwrap_or_default();
    (stats.clone().into(), telemetry)
}

pub fn run_months(world: World, months: u32) -> SimSnapshot {
    let (snap, _t) = run_months_with_telemetry(world, months);
    snap
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

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
        assert!(snap.rd_progress > 0.0);
        assert!(snap.output_units > 0u64);
        assert!(snap.revenue_usd >= Decimal::ZERO);
    }
}
