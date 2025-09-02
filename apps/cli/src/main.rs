#![deny(warnings)]

//! Headless CLI for initializing a minimal world and validating invariants.

use anyhow::Result;
use chrono::NaiveDate;
use persistence::{self, TelemetryRow};
use sim_core::*;
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

fn parse_args() -> (Option<String>, Option<u32>) {
    let mut scenario: Option<String> = None;
    let mut years: Option<u32> = None;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--scenario" => scenario = it.next(),
            "--years" => years = it.next().and_then(|s| s.parse().ok()),
            _ => {}
        }
    }
    (scenario, years)
}

fn minimal_world() -> World {
    let n800 = TechNode {
        id: TechNodeId("800nm".to_string()),
        year_available: 1990,
        density_mtr_per_mm2: rust_decimal::Decimal::new(1, 0),
        freq_ghz_baseline: rust_decimal::Decimal::new(1, 0),
        leakage_index: rust_decimal::Decimal::new(1, 0),
        yield_baseline: rust_decimal::Decimal::new(9, 1),
        wafer_cost_usd: rust_decimal::Decimal::new(1000, 0),
        mask_set_cost_usd: rust_decimal::Decimal::new(5000, 0),
        dependencies: vec![],
    };
    let n600 = TechNode {
        id: TechNodeId("600nm".to_string()),
        year_available: 1992,
        density_mtr_per_mm2: rust_decimal::Decimal::new(2, 0),
        freq_ghz_baseline: rust_decimal::Decimal::new(2, 0),
        leakage_index: rust_decimal::Decimal::new(1, 0),
        yield_baseline: rust_decimal::Decimal::new(85, 2),
        wafer_cost_usd: rust_decimal::Decimal::new(1200, 0),
        mask_set_cost_usd: rust_decimal::Decimal::new(6000, 0),
        dependencies: vec![TechNodeId("800nm".to_string())],
    };

    World {
        macro_state: MacroState {
            date: NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
            inflation_annual: 0.02,
            interest_rate: 0.05,
            fx_usd_index: 100.0,
        },
        tech_tree: vec![n800, n600],
        companies: vec![Company {
            name: "ChipCo".to_string(),
            cash_usd: rust_decimal::Decimal::new(1_000_000, 0),
            debt_usd: rust_decimal::Decimal::new(0, 0),
            ip_portfolio: vec!["uArch90s".to_string()],
        }],
        segments: vec![MarketSegment {
            name: "Desktop CPU".to_string(),
            base_demand_units: 1_000_000,
            price_elasticity: -1.2,
        }],
    }
}

fn main() -> Result<()> {
    // Logging setup
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_max_level(Level::INFO)
        .init();

    let (scenario, years) = parse_args();
    info!(?scenario, ?years, "starting CLI");

    let world = minimal_world();
    validate_world(&world)?;
    let n_companies = world.companies.len();
    let n_nodes = world.tech_tree.len();
    let n_segments = world.segments.len();

    let months = years.unwrap_or(0) * 12;
    let cfg = SimConfig {
        tick_days: 30,
        rng_seed: 42,
    };
    let ecs_world = sim_runtime::init_world(world, cfg);
    let (snap, telemetry) = sim_runtime::run_months_with_telemetry(ecs_world, months);

    println!(
        "World OK | companies: {} | tech nodes: {} | segments: {}",
        n_companies, n_nodes, n_segments
    );
    println!(
        "KPI | months: {} | cash: ${:.2} | revenue: ${:.2} | cogs: ${:.2} | contract_costs: ${:.2} | profit: ${:.2} | asp: ${:.2} | unit_cost: ${:.2} | share: {:.1}% | R&D: {:.1}% | output: {} | defects: {} | inv: {}",
        snap.months_run,
        (snap.cash_cents as f64) / 100.0,
        (snap.revenue_cents as f64) / 100.0,
        (snap.cogs_cents as f64) / 100.0,
        (snap.contract_costs_cents as f64) / 100.0,
        (snap.profit_cents as f64) / 100.0,
        (snap.asp_cents as f64) / 100.0,
        (snap.unit_cost_cents as f64) / 100.0,
        snap.market_share * 100.0,
        snap.rd_progress * 100.0,
        snap.output_units,
        snap.defect_units,
        snap.inventory_units
    );

    // Write telemetry parquet
    let rows: Vec<TelemetryRow> = telemetry
        .into_iter()
        .map(|t| TelemetryRow {
            month_index: t.month_index,
            output_units: t.output_units,
            sold_units: t.sold_units,
            asp_cents: persistence::decimal_to_cents_i64(t.asp_usd).unwrap_or(0),
            unit_cost_cents: persistence::decimal_to_cents_i64(t.unit_cost_usd).unwrap_or(0),
            margin_cents: persistence::decimal_to_cents_i64(t.margin_usd).unwrap_or(0),
            revenue_cents: persistence::decimal_to_cents_i64(t.revenue_usd).unwrap_or(0),
        })
        .collect();
    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let out_path = format!("telemetry/run_{}.parquet", ts);
    if let Err(e) = persistence::write_telemetry_parquet(&out_path, &rows) {
        eprintln!("failed to write telemetry: {e}");
    } else {
        println!("Telemetry written: {}", out_path);
    }

    Ok(())
}
