#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(warnings)]

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sim_core as core;
use sim_runtime as runtime;
use std::sync::{Arc, RwLock, Mutex};

struct SimState {
    world: runtime::World,
    dom: core::World,
    busy: bool,
}

static SIM_STATE: Lazy<Arc<RwLock<Option<SimState>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));
static TICK_QUEUE: Lazy<Arc<Mutex<()>>> = Lazy::new(|| Arc::new(Mutex::new(())));

#[derive(Serialize, Deserialize)]
struct PlanSummary {
    decisions: Vec<String>,
    expected_score: f32,
}

#[tauri::command]
async fn sim_tick(months: u32) -> Result<runtime::SimSnapshot, String> {
    let state = SIM_STATE.clone();
    let queue = TICK_QUEUE.clone();
    let snap = tauri::async_runtime::spawn_blocking(move || {
        let _q = queue.lock().unwrap();
        // Check and set busy
        {
            let mut guard = state.write().unwrap();
            let st = guard.as_mut().ok_or_else(|| "sim not initialized".to_string())?;
            if st.busy {
                return Err("busy".to_string());
            }
            st.busy = true;
        }
        // Run the tick
        let snap = {
            let mut guard = state.write().unwrap();
            let st = guard.as_mut().unwrap();
            let (snap, _t) = runtime::run_months_in_place(&mut st.world, months);
            snap
        };
        // Clear busy
        {
            let mut guard = state.write().unwrap();
            let st = guard.as_mut().unwrap();
            st.busy = false;
        }
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

#[derive(Deserialize, Debug)]
struct OverrideReq {
    price_delta_frac: Option<f32>,
    rd_delta_cents: Option<i64>,
    capacity_request: Option<CapacityReq>,
    tapeout: Option<TapeoutReq>,
}

#[derive(Deserialize, Debug)]
struct CapacityReq {
    wafers_per_month: u32,
    months: u16,
    billing_cents_per_wafer: Option<i64>,
    take_or_pay_frac: Option<f32>,
}

#[derive(Deserialize, Debug)]
struct TapeoutReq {
    perf_index: f32,
    die_area_mm2: f32,
    tech_node: String,
    expedite: Option<bool>,
}

#[derive(Serialize, Debug, Default)]
struct OverrideResp {
    asp_cents: Option<i64>,
    rd_budget_cents: Option<i64>,
    capacity_summary: Option<String>,
    tapeout_ready: Option<String>,
}

// -------- DTOs for rich state --------

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoCompany { name: String, cash_cents: i64, debt_cents: i64 }

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoSegment { name: String, base_demand_units: u64, price_elasticity: f32 }

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoPricing { asp_cents: i64, unit_cost_cents: i64 }

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoKpi {
    cash_cents: i64,
    revenue_cents: i64,
    cogs_cents: i64,
    contract_costs_cents: i64,
    profit_cents: i64,
    share: f32,
    rd_pct: f32,
    output_units: u64,
    inventory_units: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoContract {
    foundry_id: String,
    wafers_per_month: u32,
    billing_cents_per_wafer: i64,
    take_or_pay_frac: f32,
    start: String,
    end: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoTapeoutReq {
    tech_node: String,
    start: String,
    ready: String,
    expedite: bool,
    expedite_cost_cents: i64,
    perf_index: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoPipeline { queue: Vec<DtoTapeoutReq>, released: Vec<core::ProductSpec> }

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoConfig {
    finance: runtime::FinanceConfig,
    product_cost: sim_ai::ProductCostCfg,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SimStateDto {
    date: String,
    month_index: u32,
    companies: Vec<DtoCompany>,
    segments: Vec<DtoSegment>,
    pricing: DtoPricing,
    kpi: DtoKpi,
    contracts: Vec<DtoContract>,
    pipeline: DtoPipeline,
    ai_plan: PlanSummary,
    config: DtoConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SimListsDto {
    tech_nodes: Vec<String>,
    foundries: Vec<String>,
    segments: Vec<String>,
}

fn build_sim_state_dto(st: &SimState) -> SimStateDto {
    let world = &st.world;
    let dom = &st.dom;
    let date = dom.macro_state.date.to_string();
    let stats = world.resource::<runtime::Stats>();
    let pricing = world.resource::<runtime::Pricing>();
    let kpi = DtoKpi {
        cash_cents: persistence::decimal_to_cents_i64(dom.companies[0].cash_usd).unwrap_or(0),
        revenue_cents: persistence::decimal_to_cents_i64(stats.revenue_usd).unwrap_or(0),
        cogs_cents: persistence::decimal_to_cents_i64(stats.cogs_usd).unwrap_or(0),
        contract_costs_cents: stats.contract_costs_cents,
        profit_cents: persistence::decimal_to_cents_i64(stats.profit_usd).unwrap_or(0),
        share: stats.market_share,
        rd_pct: stats.rd_progress,
        output_units: stats.output_units,
        inventory_units: stats.inventory_units,
    };
    let companies = dom
        .companies
        .iter()
        .map(|c| DtoCompany {
            name: c.name.clone(),
            cash_cents: persistence::decimal_to_cents_i64(c.cash_usd).unwrap_or(0),
            debt_cents: persistence::decimal_to_cents_i64(c.debt_usd).unwrap_or(0),
        })
        .collect();
    let segments = dom
        .segments
        .iter()
        .map(|s| DtoSegment { name: s.name.clone(), base_demand_units: s.base_demand_units, price_elasticity: s.price_elasticity })
        .collect();
    let book = world.resource::<runtime::CapacityBook>();
    let contracts = book
        .contracts
        .iter()
        .map(|c| DtoContract {
            foundry_id: c.foundry_id.clone(),
            wafers_per_month: c.wafers_per_month,
            billing_cents_per_wafer: c.billing_cents_per_wafer,
            take_or_pay_frac: c.take_or_pay_frac,
            start: c.start.to_string(),
            end: c.end.to_string(),
        })
        .collect();
    let pipe = world.resource::<runtime::Pipeline>();
    let mut queue: Vec<DtoTapeoutReq> = Vec::new();
    for t in &pipe.0.queue {
        queue.push(DtoTapeoutReq {
            tech_node: t.tech_node.0.clone(),
            start: t.start.to_string(),
            ready: t.ready.to_string(),
            expedite: t.expedite,
            expedite_cost_cents: t.expedite_cost_cents,
            perf_index: t.product.perf_index,
        });
    }
    let released = pipe.0.released.clone();
    let ai_cfg = world.resource::<runtime::AiConfig>().0.clone();
    let asp_cents = persistence::decimal_to_cents_i64(pricing.asp_usd).unwrap_or(0);
    let unit_cost_cents = persistence::decimal_to_cents_i64(pricing.unit_cost_usd).unwrap_or(0);
    SimStateDto {
        date,
        month_index: stats.months_run,
        companies,
        segments,
        pricing: DtoPricing { asp_cents, unit_cost_cents },
        kpi,
        contracts,
        pipeline: DtoPipeline { queue, released },
        ai_plan: PlanSummary { decisions: vec!["n/a".into()], expected_score: 0.0 },
        config: DtoConfig { finance: *world.resource::<runtime::FinanceConfig>(), product_cost: ai_cfg.product_cost },
    }
}

#[tauri::command]
fn sim_state() -> Result<SimStateDto, String> {
    let guard = SIM_STATE.read().unwrap();
    let st = guard.as_ref().ok_or_else(|| "sim not initialized".to_string())?;
    Ok(build_sim_state_dto(st))
}

#[tauri::command]
fn sim_lists() -> Result<SimListsDto, String> {
    let guard = SIM_STATE.read().unwrap();
    let st = guard.as_ref().ok_or_else(|| "sim not initialized".to_string())?;
    let tech_nodes = st
        .dom
        .tech_tree
        .iter()
        .map(|n| n.id.0.clone())
        .collect::<Vec<_>>();
    let foundries = st
        .world
        .resource::<runtime::CapacityBook>()
        .contracts
        .iter()
        .map(|c| c.foundry_id.clone())
        .collect::<Vec<_>>();
    let segments = st.dom.segments.iter().map(|s| s.name.clone()).collect();
    Ok(SimListsDto { tech_nodes, foundries, segments })
}

#[tauri::command]
fn sim_override(ovr: OverrideReq) -> Result<OverrideResp, String> {
    let mut resp = OverrideResp::default();
    let state = SIM_STATE.clone();
    let mut guard = state.write().unwrap();
    let st = guard.as_mut().ok_or_else(|| "sim not initialized".to_string())?;
    let world = &mut st.world;
    if let Some(df) = ovr.price_delta_frac {
        let asp = runtime::apply_price_delta(world, df);
        resp.asp_cents = Some(persistence::decimal_to_cents_i64(asp).unwrap_or(0));
    }
    if let Some(d) = ovr.rd_delta_cents {
        let b = runtime::apply_rd_delta(world, d);
        resp.rd_budget_cents = Some(b);
    }
    if let Some(cap) = ovr.capacity_request {
        let s = runtime::apply_capacity_request(
            world,
            cap.wafers_per_month,
            cap.months,
            cap.billing_cents_per_wafer,
            cap.take_or_pay_frac,
        );
        resp.capacity_summary = Some(s);
    }
    if let Some(t) = ovr.tapeout {
        let ready = runtime::apply_tapeout_request(
            world,
            t.perf_index,
            t.die_area_mm2,
            t.tech_node,
            t.expedite.unwrap_or(false),
        );
        resp.tapeout_ready = Some(ready.to_string());
    }
    Ok(resp)
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
    *SIM_STATE.write().unwrap() = Some(SimState { world: ecs, dom, busy: false });

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![sim_tick, sim_plan_quarter, sim_override, sim_state, sim_lists])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_ticks_increase_month_index() {
        // Initialize state
        let dom = core::World {
            macro_state: core::MacroState { date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(), inflation_annual: 0.02, interest_rate: 0.05, fx_usd_index: 100.0 },
            tech_tree: vec![],
            companies: vec![core::Company { name: "A".into(), cash_usd: rust_decimal::Decimal::new(1_000_000, 0), debt_usd: rust_decimal::Decimal::ZERO, ip_portfolio: vec![] }],
            segments: vec![core::MarketSegment { name: "Seg".into(), base_demand_units: 1_000_000, price_elasticity: -1.2 }],
        };
        let ecs = runtime::init_world(dom.clone(), core::SimConfig { tick_days: 30, rng_seed: 42 });
        *SIM_STATE.write().unwrap() = Some(SimState { world: ecs, dom, busy: false });
        // Run two ticks sequentially
        let rt = tauri::async_runtime::TokioRuntime::new().expect("rt");
        let s1 = rt.block_on(sim_tick(1)).expect("tick1");
        let s2 = rt.block_on(sim_tick(1)).expect("tick2");
        assert!(s2.months_run > s1.months_run);
    }

    #[test]
    fn second_tick_returns_busy_status() {
        // Initialize state
        let dom = core::World {
            macro_state: core::MacroState { date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(), inflation_annual: 0.02, interest_rate: 0.05, fx_usd_index: 100.0 },
            tech_tree: vec![],
            companies: vec![core::Company { name: "A".into(), cash_usd: rust_decimal::Decimal::new(1_000_000, 0), debt_usd: rust_decimal::Decimal::ZERO, ip_portfolio: vec![] }],
            segments: vec![core::MarketSegment { name: "Seg".into(), base_demand_units: 1_000_000, price_elasticity: -1.2 }],
        };
        let ecs = runtime::init_world(dom.clone(), core::SimConfig { tick_days: 30, rng_seed: 42 });
        *SIM_STATE.write().unwrap() = Some(SimState { world: ecs, dom, busy: true });
        // Try tick while busy
        let rt = tauri::async_runtime::TokioRuntime::new().expect("rt");
        let res = rt.block_on(sim_tick(1));
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "busy");
        // Clear and tick OK
        {
            let mut g = SIM_STATE.write().unwrap();
            g.as_mut().unwrap().busy = false;
        }
        let _ = rt.block_on(sim_tick(1)).expect("tick ok");
    }

    #[test]
    fn overrides_apply_and_affect_state() {
        // Init state with a tech node for tapeout
        let dom = core::World {
            macro_state: core::MacroState { date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(), inflation_annual: 0.02, interest_rate: 0.05, fx_usd_index: 100.0 },
            tech_tree: vec![core::TechNode { id: core::TechNodeId("N90".into()), year_available: 1990, density_mtr_per_mm2: rust_decimal::Decimal::new(1,0), freq_ghz_baseline: rust_decimal::Decimal::new(1,0), leakage_index: rust_decimal::Decimal::new(1,0), yield_baseline: rust_decimal::Decimal::new(9,1), wafer_cost_usd: rust_decimal::Decimal::new(1000,0), mask_set_cost_usd: rust_decimal::Decimal::new(5000,0), dependencies: vec![] }],
            companies: vec![core::Company { name: "A".into(), cash_usd: rust_decimal::Decimal::new(1_000_000, 0), debt_usd: rust_decimal::Decimal::ZERO, ip_portfolio: vec![] }],
            segments: vec![core::MarketSegment { name: "Seg".into(), base_demand_units: 1_000_000, price_elasticity: -1.2 }],
        };
        let ecs = runtime::init_world(dom.clone(), core::SimConfig { tick_days: 30, rng_seed: 42 });
        *SIM_STATE.write().unwrap() = Some(SimState { world: ecs, dom, busy: false });

        // Apply price +5%
        let r = sim_override(OverrideReq { price_delta_frac: Some(0.05), rd_delta_cents: None, capacity_request: None, tapeout: None }).expect("override");
        assert!(r.asp_cents.unwrap_or(0) > 0);

        // Apply R&D budget increase
        let _ = sim_override(OverrideReq { price_delta_frac: None, rd_delta_cents: Some(10_000), capacity_request: None, tapeout: None }).expect("rd");
        {
            let g = SIM_STATE.read().unwrap();
            let world = &g.as_ref().unwrap().world;
            let b = world.resource::<runtime::RnDBudgetCents>().0;
            assert_eq!(b, 10_000);
        }

        // Capacity request
        let _ = sim_override(OverrideReq { price_delta_frac: None, rd_delta_cents: None, capacity_request: Some(CapacityReq { wafers_per_month: 1000, months: 12, billing_cents_per_wafer: Some(5000), take_or_pay_frac: Some(1.0) }), tapeout: None }).expect("cap");
        {
            let g = SIM_STATE.read().unwrap();
            let world = &g.as_ref().unwrap().world;
            assert!(!world.resource::<runtime::CapacityBook>().contracts.is_empty());
        }

        // Tapeout expedited, then tick to ready and expect release
        let resp = sim_override(OverrideReq { price_delta_frac: None, rd_delta_cents: None, capacity_request: None, tapeout: Some(TapeoutReq { perf_index: 0.8, die_area_mm2: 100.0, tech_node: "N90".into(), expedite: Some(true) }) }).expect("tapeout");
        let ready = chrono::NaiveDate::parse_from_str(&resp.tapeout_ready.unwrap(), "%Y-%m-%d").unwrap();
        // Compute months to ready from current date
        let start = SIM_STATE.read().unwrap().as_ref().unwrap().dom.macro_state.date;
        let mut months = 0u32;
        let mut d = start;
        while d < ready && months < 24 {
            d = runtime::add_months(d, 1);
            months += 1;
        }
        let rt = tauri::async_runtime::TokioRuntime::new().expect("rt");
        let _ = rt.block_on(sim_tick(months)).expect("tick to ready");
        let g = SIM_STATE.read().unwrap();
        let world = &g.as_ref().unwrap().world;
        assert!(!world.resource::<runtime::Pipeline>().0.released.is_empty());
    }

    #[test]
    fn state_dto_roundtrip_and_updates() {
        let dom = core::World {
            macro_state: core::MacroState { date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(), inflation_annual: 0.02, interest_rate: 0.05, fx_usd_index: 100.0 },
            tech_tree: vec![core::TechNode { id: core::TechNodeId("N90".into()), year_available: 1990, density_mtr_per_mm2: rust_decimal::Decimal::new(1,0), freq_ghz_baseline: rust_decimal::Decimal::new(1,0), leakage_index: rust_decimal::Decimal::new(1,0), yield_baseline: rust_decimal::Decimal::new(9,1), wafer_cost_usd: rust_decimal::Decimal::new(1000,0), mask_set_cost_usd: rust_decimal::Decimal::new(5000,0), dependencies: vec![] }],
            companies: vec![core::Company { name: "A".into(), cash_usd: rust_decimal::Decimal::new(1_000_000, 0), debt_usd: rust_decimal::Decimal::ZERO, ip_portfolio: vec![] }],
            segments: vec![core::MarketSegment { name: "Seg".into(), base_demand_units: 1_000_000, price_elasticity: -1.2 }],
        };
        let ecs = runtime::init_world(dom.clone(), core::SimConfig { tick_days: 30, rng_seed: 42 });
        *SIM_STATE.write().unwrap() = Some(SimState { world: ecs, dom, busy: false });
        // Initial state
        let s1 = sim_state().expect("state");
        let js = serde_json::to_string(&s1).expect("ser");
        let back: SimStateDto = serde_json::from_str(&js).expect("de");
        assert_eq!(back.month_index, 0);
        // Tick and state must update
        let rt = tauri::async_runtime::TokioRuntime::new().expect("rt");
        let _ = rt.block_on(sim_tick(1)).expect("tick");
        let s2 = sim_state().expect("state2");
        assert!(s2.month_index > s1.month_index);
        // Price override updates pricing in dto
        let _ = sim_override(OverrideReq { price_delta_frac: Some(0.05), rd_delta_cents: None, capacity_request: None, tapeout: None }).unwrap();
        let s3 = sim_state().unwrap();
        assert!(s3.pricing.asp_cents >= s2.pricing.asp_cents);
    }
}
