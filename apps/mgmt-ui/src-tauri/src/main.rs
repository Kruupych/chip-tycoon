#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(warnings)]

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sim_core as core;
use sim_runtime as runtime;
use std::sync::Mutex;

static SIM_STATE: Lazy<Mutex<Option<(runtime::World, core::World)>>> = Lazy::new(|| Mutex::new(None));

#[derive(Serialize, Deserialize)]
struct PlanSummary {
    decisions: Vec<String>,
    expected_score: f32,
}

#[tauri::command]
fn sim_tick(months: u32) -> Result<runtime::SimSnapshot, String> {
    let mut guard = SIM_STATE.lock().unwrap();
    let (ecs_world, _dom) = guard
        .as_mut()
        .ok_or_else(|| "sim not initialized".to_string())?;
    let (snap, _t) = runtime::run_months_with_telemetry(std::mem::take(ecs_world.clone()), months);
    // Re-init world after run to keep state consistent for subsequent ticks
    *ecs_world = runtime::init_world(_dom.clone(), core::SimConfig { tick_days: 30, rng_seed: 42 });
    Ok(snap)
}

#[tauri::command]
fn sim_plan_quarter() -> Result<PlanSummary, String> {
    let guard = SIM_STATE.lock().unwrap();
    let (ecs_world, dom) = guard.as_ref().ok_or_else(|| "sim not initialized".to_string())?;
    let stats = ecs_world.get_resource::<runtime::Stats>().ok_or("no stats").map_err(|_| "stats missing".to_string());
    let pricing = ecs_world.get_resource::<runtime::Pricing>().ok_or("no pricing").map_err(|_| "pricing missing".to_string());
    drop(stats);
    drop(pricing);
    // Return a placeholder summary for now
    Ok(PlanSummary { decisions: vec!["AdjustPrice(-5%)".into()], expected_score: 0.5 })
}

#[derive(Deserialize)]
struct OverrideReq {
    pricing: Option<f32>,
    rd_delta: Option<f32>,
    capacity_request: Option<u64>,
}

#[tauri::command]
fn sim_override(_ovr: OverrideReq) -> Result<String, String> {
    Ok("ok".into())
}

fn main() {
    // Init a basic world on startup
    let dom = core::World {
        macro_state: core::MacroState { date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(), inflation_annual: 0.02, interest_rate: 0.05, fx_usd_index: 100.0 },
        tech_tree: vec![],
        companies: vec![core::Company { name: "A".into(), cash_usd: rust_decimal::Decimal::new(1_000_000, 0), debt_usd: rust_decimal::Decimal::ZERO, ip_portfolio: vec![] }],
        segments: vec![core::MarketSegment { name: "Seg".into(), base_demand_units: 1_000_000, price_elasticity: -1.2 }],
    };
    let ecs = runtime::init_world(dom.clone(), core::SimConfig { tick_days: 30, rng_seed: 42 });
    *SIM_STATE.lock().unwrap() = Some((ecs, dom));

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![sim_tick, sim_plan_quarter, sim_override])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

