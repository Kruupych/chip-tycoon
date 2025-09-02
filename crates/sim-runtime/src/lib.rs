#![deny(warnings)]

//! ECS runtime for the simulation.
//!
//! Exposes a simple monthly tick runner with deterministic, stubbed systems.

use bevy_ecs::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rust_decimal::{prelude::ToPrimitive, Decimal};
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
    pub market_share: f32,
    pub last_share: f32,
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

/// Pricing resource to allow AI to adjust ASP while sales reads it.
#[derive(Resource, Clone)]
pub struct Pricing {
    pub asp_usd: Decimal,
    pub unit_cost_usd: Decimal,
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

/// Global RNG resource seeded from `SimConfig` for deterministic noise.
#[derive(Resource)]
pub struct RngResource(pub ChaCha8Rng);

/// Foundry capacity contracts.
#[derive(Clone, Debug)]
pub struct FoundryContract {
    pub foundry_id: String,
    pub wafers_per_month: u32,
    pub price_per_wafer_cents: i64,
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

/// Sales system: sells some inventory at a fixed ASP and cost.
pub fn sales_system(mut stats: ResMut<Stats>, pricing: Res<Pricing>) {
    let sell_units = (stats.inventory_units as f64 * 0.5) as u64; // sell 50%
    let revenue = pricing.asp_usd * Decimal::from(sell_units);
    let cost = pricing.unit_cost_usd * Decimal::from(sell_units);
    let profit = revenue - cost;
    stats.revenue_usd += revenue;
    stats.profit_usd += profit;
    stats.inventory_units = stats.inventory_units.saturating_sub(sell_units);
    info!(target: "sim.sales", sell_units, revenue = %stats.revenue_usd, profit = %stats.profit_usd, asp = %pricing.asp_usd, "Sales updated");
}

/// Finance system: placeholder for interests, cash flow, etc.
pub fn finance_system(stats: ResMut<Stats>) {
    // Simple carry-over, ensure profit is not NaN (never is for Decimal).
    info!(target: "sim.finance", profit = %stats.profit_usd, "Finance tick");
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
    let a = (r / p).powf(beta);
    let target_share = (a / (a + comp_attr)).clamp(0.05, 0.95);
    stats.market_share += (target_share - stats.market_share) * 0.1;
    stats.market_share = stats.market_share.clamp(0.05, 0.95);

    // Quarterly planning signal (every 3 months)
    if (stats.months_run + 1) % 3 == 0 {
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
                        * rust_decimal::Decimal::from_f32_retain(
                            1.0 + cfg.0.tactics.min_margin_frac,
                        )
                        .unwrap_or(Decimal::ONE);
                    if np < minp {
                        np = minp;
                    }
                    pricing.asp_usd = np;
                }
                ai::PlanAction::AllocateRndBoost(db) => {
                    stats.rd_progress = (stats.rd_progress + db).clamp(0.0, 1.0);
                }
                ai::PlanAction::RequestCapacity(_u) => {
                    // No direct hook into foundry capacity in this simplified runtime.
                }
            }
        }
    }

    // Update last_share tracker
    stats.last_share = stats.market_share;
    info!(target: "sim.ai", share = stats.market_share, asp = %pricing.asp_usd, rnd = stats.rd_progress, "AI strategy updated");
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
        let pricing = world.resource::<Pricing>().clone();
        let mut stats = world.resource_mut::<Stats>();
        stats.months_run = stats.months_run.saturating_add(1);
        let sold_units = (stats.inventory_units as f64 * 0.5) as u64; // mirrors sales_system
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
        assert!(snap.rd_progress >= 0.0);
        assert!(snap.output_units > 0u64);
        assert!(snap.revenue_usd >= Decimal::ZERO);
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
        assert_eq!(snap1.revenue_usd, snap2.revenue_usd);
        assert_eq!(snap1.profit_usd, snap2.profit_usd);
        assert!((snap1.market_share - snap2.market_share).abs() < f32::EPSILON);
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
    fn capacity_contract_increases_after_lead_time() {
        use chrono::Datelike;
        let dom = core::World {
            macro_state: core::MacroState { date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(), inflation_annual: 0.02, interest_rate: 0.05, fx_usd_index: 100.0 },
            tech_tree: vec![],
            companies: vec![core::Company { name: "A".into(), cash_usd: Decimal::new(1_000_000, 0), debt_usd: Decimal::ZERO, ip_portfolio: vec![] }],
            segments: vec![core::MarketSegment { name: "Seg".into(), base_demand_units: 1_000_000, price_elasticity: -1.2 }],
        };
        let cfg = core::SimConfig { tick_days: 30, rng_seed: 1 };
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
        if m > 12 { y += 1; m -= 12; }
        let start_plus_2 = chrono::NaiveDate::from_ymd_opt(y, m, start.day()).unwrap();
        {
            let mut book = w.resource_mut::<CapacityBook>();
            book.contracts.push(FoundryContract {
                foundry_id: "F1".into(),
                wafers_per_month: 500,
                price_per_wafer_cents: 100_00,
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
}
