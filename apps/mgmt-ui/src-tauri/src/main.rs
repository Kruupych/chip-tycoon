#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(warnings)]

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sim_core as core;
use sim_runtime as runtime;
use std::sync::{Arc, RwLock};

static SIM_STATE: Lazy<Arc<RwLock<Option<(runtime::World, core::World)>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

#[derive(Serialize, Deserialize)]
struct PlanSummary {
    decisions: Vec<String>,
    expected_score: f32,
}

#[tauri::command]
async fn sim_tick(months: u32) -> Result<runtime::SimSnapshot, String> {
    let state = SIM_STATE.clone();
    let snap = tauri::async_runtime::spawn_blocking(move || {
        let mut guard = state.write().unwrap();
        let (world, _dom) = guard.as_mut().ok_or_else(|| "sim not initialized".to_string())?;
        let (snap, _t) = runtime::run_months_in_place(world, months);
        Ok::<_, String>(snap)
    })
    .await
    .map_err(|e| format!("join error: {e}"))??;
    Ok(snap)
}

#[tauri::command]
async fn sim_plan_quarter() -> Result<PlanSummary, String> {
    // Placeholder implementation
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
    *SIM_STATE.write().unwrap() = Some((ecs, dom));

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![sim_tick, sim_plan_quarter, sim_override])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
