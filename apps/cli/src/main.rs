#![deny(warnings)]

//! Headless CLI for initializing a minimal world and validating invariants.

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use persistence::{self, TelemetryRow};
use sim_core::*;
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

fn parse_args() -> (Option<String>, Option<u32>, Option<String>, Option<String>) {
    let mut scenario: Option<String> = None;
    let mut years: Option<u32> = None;
    let mut campaign: Option<String> = None;
    let mut export_path: Option<String> = None;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--scenario" => scenario = it.next(),
            "--years" => years = it.next().and_then(|s| s.parse().ok()),
            "--campaign" => campaign = it.next(),
            "--export-campaign" => export_path = it.next(),
            _ => {}
        }
    }
    (scenario, years, campaign, export_path)
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

#[derive(serde::Deserialize)]
struct CampaignScenario {
    start_date: String,
    end_date: String,
    player_start_cash_cents: I64OrStr,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum I64OrStr {
    I(i64),
    S(String),
}
impl I64OrStr {
    fn val(&self) -> i64 {
        match self {
            I64OrStr::I(i) => *i,
            I64OrStr::S(s) => s.replace('_', "").parse::<i64>().unwrap_or(0),
        }
    }
}

fn main() -> Result<()> {
    // Logging setup
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_max_level(Level::INFO)
        .init();

    let (scenario, years, campaign, export_path) = parse_args();
    info!(?scenario, ?years, ?campaign, ?export_path, "starting CLI");

    if let Some(camp) = campaign {
        // Load campaign and run to completion
        let path = if camp == "1990s" {
            "assets/scenarios/campaign_1990s.yaml".to_string()
        } else {
            camp
        };
        let text = std::fs::read_to_string(&path)?;
        let sc: CampaignScenario = serde_yaml::from_str(&text)?;
        let start = chrono::NaiveDate::parse_from_str(&sc.start_date, "%Y-%m-%d")?;
        let end = chrono::NaiveDate::parse_from_str(&sc.end_date, "%Y-%m-%d")?;
        let months = ((end.year() - start.year()) * 12
            + (end.month() as i32 - start.month() as i32))
            .max(0) as u32;
        let mut world = minimal_world();
        world.macro_state.date = start;
        if let Some(c) = world.companies.first_mut() {
            c.cash_usd = persistence::cents_i64_to_decimal(sc.player_start_cash_cents.val());
        }
        let cfg = SimConfig {
            tick_days: 30,
            rng_seed: 42,
        };
        let mut ecs = sim_runtime::init_world(world, cfg);
        // Load 1990s assets into runtime for richer balance
        let markets =
            sim_runtime::MarketConfigRes::from_yaml_file("assets/data/markets_1990s.yaml")
                .unwrap_or_default();
        ecs.insert_resource(markets);
        ecs.insert_resource(sim_runtime::load_market_events_yaml(
            "assets/events/campaign_1990s.yaml",
        ));
        // Export monthly timeline if requested
        if let Some(path) = &export_path {
            #[derive(serde::Serialize)]
            struct Row {
                date: String,
                month_index: u32,
                cash_cents: i64,
                revenue_cents: i64,
                cogs_cents: i64,
                profit_cents: i64,
                asp_cents: i64,
                unit_cost_cents: i64,
                share: f32,
                output_units: u64,
                inventory_units: u64,
                active_mods: Vec<String>,
                goals: Vec<String>,
            }
            let mut rows: Vec<Row> = Vec::with_capacity(months as usize);
            for _ in 0..months {
                let (_snap, _t) = sim_runtime::run_months_in_place(&mut ecs, 1);
                let dom = ecs.resource::<sim_runtime::DomainWorld>();
                let stats = ecs.resource::<sim_runtime::Stats>();
                let pricing = ecs.resource::<sim_runtime::Pricing>();
                let date = dom.0.macro_state.date;
                // Active mods summary
                let mut active_list: Vec<String> = Vec::new();
                if let Some(me) = ecs.get_non_send_resource::<sim_runtime::ModEngineRes>() {
                    for (id, start, end) in me.engine.active_effects_summary() {
                        if date >= start && date < end {
                            active_list.push(id.clone());
                        }
                    }
                }
                if let Some(mm) = ecs.get_resource::<sim_runtime::MarketModEffects>() {
                    for e in &mm.0 {
                        if date >= e.start && date < e.end {
                            active_list.push(e.id.clone());
                        }
                    }
                }
                // Goals status strings
                let mut goals: Vec<String> = Vec::new();
                if let Some(st) = ecs.get_resource::<sim_runtime::CampaignStateRes>() {
                    for (i, stg) in st.goal_status.iter().enumerate() {
                        goals.push(format!("g{}:{:?}", i, stg));
                    }
                }
                rows.push(Row {
                    date: date.to_string(),
                    month_index: stats.months_run,
                    cash_cents: persistence::decimal_to_cents_i64(dom.0.companies[0].cash_usd)
                        .unwrap_or(0),
                    revenue_cents: persistence::decimal_to_cents_i64(stats.revenue_usd)
                        .unwrap_or(0),
                    cogs_cents: persistence::decimal_to_cents_i64(stats.cogs_usd).unwrap_or(0),
                    profit_cents: persistence::decimal_to_cents_i64(stats.profit_usd).unwrap_or(0),
                    asp_cents: persistence::decimal_to_cents_i64(pricing.asp_usd).unwrap_or(0),
                    unit_cost_cents: persistence::decimal_to_cents_i64(pricing.unit_cost_usd)
                        .unwrap_or(0),
                    share: stats.market_share,
                    output_units: stats.output_units,
                    inventory_units: stats.inventory_units,
                    active_mods: active_list,
                    goals,
                });
            }
            if path.ends_with(".json") {
                std::fs::create_dir_all(
                    std::path::Path::new(path)
                        .parent()
                        .unwrap_or(std::path::Path::new(".")),
                )?;
                let s = serde_json::to_string_pretty(&rows)?;
                std::fs::write(path, s)?;
                println!(
                    "Campaign exported to JSON: {} ({} months)",
                    path,
                    rows.len()
                );
            } else if path.ends_with(".parquet") {
                // Convert to TelemetryRow and write (goals/mods not included in parquet)
                let mut trows: Vec<persistence::TelemetryRow> = Vec::with_capacity(rows.len());
                for r in rows.iter() {
                    trows.push(persistence::TelemetryRow {
                        month_index: r.month_index,
                        output_units: r.output_units,
                        sold_units: 0,
                        asp_cents: r.asp_cents,
                        unit_cost_cents: r.unit_cost_cents,
                        margin_cents: r.profit_cents,
                        revenue_cents: r.revenue_cents,
                    });
                }
                persistence::write_telemetry_parquet(path, &trows)?;
                println!(
                    "Campaign exported to Parquet: {} ({} months)",
                    path,
                    trows.len()
                );
            } else {
                eprintln!(
                    "Unknown export extension for {}. Use .json or .parquet",
                    path
                );
            }
        } else {
            // Default run: just to completion
            let (snap, _t) = sim_runtime::run_months_in_place(&mut ecs, months);
            println!(
                "KPI | date: {} | months: {} | cash: ${:.2} | profit: ${:.2}",
                ecs.resource::<sim_runtime::DomainWorld>()
                    .0
                    .macro_state
                    .date,
                snap.months_run,
                (snap.cash_cents as f64) / 100.0,
                (snap.profit_cents as f64) / 100.0,
            );
            println!(
                "Campaign 1990s — result: {}",
                if snap.profit_cents > 0 {
                    "Success"
                } else {
                    "InProgress"
                }
            );
        }
        return Ok(());
    }

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
    let mut ecs_world = sim_runtime::init_world(world, cfg);
    let (snap, telemetry) = sim_runtime::run_months_in_place(&mut ecs_world, months);

    println!(
        "World OK | companies: {} | tech nodes: {} | segments: {}",
        n_companies, n_nodes, n_segments
    );
    println!(
        "KPI | date: {} | months: {} | cash: ${:.2} | revenue: ${:.2} | cogs: ${:.2} | contract_costs: ${:.2} | profit: ${:.2} | asp: ${:.2} | unit_cost: ${:.2} | share: {:.1}% | R&D: {:.1}% | output: {} | defects: {} | inv: {}",
        ecs_world.resource::<sim_runtime::DomainWorld>().0.macro_state.date,
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
