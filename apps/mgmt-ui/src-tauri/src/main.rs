#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(warnings)]

use chrono::Datelike;
use tauri::Manager; // for AppHandle.path()
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sim_core as core;
use sim_runtime as runtime;
use std::sync::{Arc, Mutex, RwLock};
mod embedded;

fn validate_yaml<T: for<'de> Deserialize<'de> + JsonSchema>(
    yaml_text: &str,
    _schema_name: &str,
) -> Result<(), String> {
    // Build schema from type
    let schema = schemars::schema_for!(T);
    let schema_json = serde_json::to_value(&schema).map_err(|e| e.to_string())?;
    // Optionally persist schema under assets/schema
    // Best-effort write in debug builds only; avoid unused variables in release.
    #[cfg(debug_assertions)]
    {
        if let Ok(s) = serde_json::to_string_pretty(&schema_json) {
            let out_path = format!("assets/schema/{}.json", _schema_name);
            let _ = std::fs::create_dir_all("assets/schema");
            let _ = std::fs::write(&out_path, s);
        }
    }
    // Parse YAML as JSON value
    let data_yaml: serde_yaml::Value =
        serde_yaml::from_str(yaml_text).map_err(|e| e.to_string())?;
    let data_json = serde_json::to_value(data_yaml).map_err(|e| e.to_string())?;
    // Validate
    let compiled = jsonschema::JSONSchema::compile(&schema_json).map_err(|e| e.to_string())?;
    if let Err(errors) = compiled.validate(&data_json) {
        // Build human-readable error list
        let mut msgs: Vec<String> = Vec::new();
        for err in errors {
            let path = err.instance_path.to_string();
            msgs.push(format!(
                "{}: {}",
                if path.is_empty() { "/".into() } else { path },
                err
            ));
        }
        let joined = msgs.join("; ");
        return Err(joined);
    }
    Ok(())
}

struct SimState {
    world: runtime::World,
    dom: core::World,
    busy: bool,
    scenario: Option<CampaignScenario>,
    tutorial: Option<TutorialCfg>,
    autosave: bool,
}

static SIM_STATE: Lazy<Arc<RwLock<Option<SimState>>>> = Lazy::new(|| Arc::new(RwLock::new(None)));
static TICK_QUEUE: Lazy<Arc<Mutex<()>>> = Lazy::new(|| Arc::new(Mutex::new(())));

#[derive(Serialize, Deserialize, Debug, Clone)]
struct PlanSummary {
    decisions: Vec<String>,
    expected_score: f32,
}

#[tauri::command]
async fn sim_tick(app: tauri::AppHandle, months: u32) -> Result<runtime::SimSnapshot, String> {
    tracing::info!(target: "ipc", months, "sim_tick");
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = app.run_on_main_thread(move || {
        let state = SIM_STATE.clone();
        let queue = TICK_QUEUE.clone();
        let res = (|| {
            let _q = queue.lock().unwrap();
            {
                let mut guard = state.write().unwrap();
                let st = guard
                    .as_mut()
                    .ok_or_else(|| "sim not initialized".to_string())?;
                if st.busy {
                    return Err("busy".to_string());
                }
                st.busy = true;
            }
            let snap = {
                let mut guard = state.write().unwrap();
                let st = guard.as_mut().unwrap();
                let (snap, _t) = runtime::run_months_in_place(&mut st.world, months);
                snap
            };
            {
                let mut guard = state.write().unwrap();
                let st = guard.as_mut().unwrap();
                st.busy = false;
            }
            Ok::<_, String>(snap)
        })();
        let _ = tx.send(res);
    });
    let snap = rx.recv().map_err(|e| e.to_string())??;
    tracing::info!(target: "ipc", months_run = snap.months_run, "sim_tick: ok");
    Ok(snap)
}

#[tauri::command]
async fn sim_tick_quarter(app: tauri::AppHandle) -> Result<runtime::SimSnapshot, String> {
    tracing::info!(target: "ipc", "sim_tick_quarter");
    // Precompute autosave DB URL (to avoid borrowing `app` inside main-thread closure)
    let db_url_opt = saves_db_url(&app).ok();
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = app.run_on_main_thread(move || {
        let state = SIM_STATE.clone();
        let queue = TICK_QUEUE.clone();
        let res = (|| {
            let _q = queue.lock().unwrap();
            {
                let mut guard = state.write().unwrap();
                let st = guard
                    .as_mut()
                    .ok_or_else(|| "sim not initialized".to_string())?;
                if st.busy {
                    return Err("busy".to_string());
                }
                st.busy = true;
            }
            let snap = {
                let mut guard = state.write().unwrap();
                let st = guard.as_mut().unwrap();
                let (_s1, _t1) = runtime::run_months_in_place(&mut st.world, 1);
                let (_s2, _t2) = runtime::run_months_in_place(&mut st.world, 1);
                let (s3, _t3) = runtime::run_months_in_place(&mut st.world, 1);
                if st.autosave {
                    let date = st
                        .world
                        .resource::<runtime::DomainWorld>()
                        .0
                        .macro_state
                        .date;
                    let name = format!("auto-{}{:02}", date.year(), date.month());
                    let dom_clone = st.dom.clone();
                    let world_clone = runtime::clone_world_state(&st.world);
                    if let Some(db_url) = db_url_opt.clone() {
                        tauri::async_runtime::spawn(async move {
                            let _ = save_now(db_url, name, dom_clone, world_clone).await;
                        });
                    } else {
                        tracing::error!(target: "ipc", "autosave: db url error");
                    }
                }
                s3
            };
            {
                let mut guard = state.write().unwrap();
                let st = guard.as_mut().unwrap();
                st.busy = false;
            }
            Ok::<_, String>(snap)
        })();
        let _ = tx.send(res);
    });
    let snap = rx.recv().map_err(|e| e.to_string())??;
    tracing::info!(target: "ipc", months_run = snap.months_run, "sim_tick_quarter: ok");
    Ok(snap)
}

#[tauri::command]
async fn sim_plan_quarter() -> Result<PlanSummary, String> {
    let guard = SIM_STATE.read().unwrap();
    let st = guard
        .as_ref()
        .ok_or_else(|| "sim not initialized".to_string())?;
    let world = &st.world;
    let dom = &st.dom;
    // Derive current KPIs for planner
    let stats = world.resource::<runtime::Stats>();
    let pricing = world.resource::<runtime::Pricing>();
    // Approximate monthly good-unit capacity (if Capacity present, else baseline)
    let cap = world
        .get_resource::<runtime::Capacity>()
        .map(|c| c.wafers_per_month * 50 - (c.wafers_per_month * 50) / 20)
        .unwrap_or(1_000_000);
    let current = sim_ai::CurrentKpis {
        asp_usd: pricing.asp_usd,
        unit_cost_usd: pricing.unit_cost_usd,
        capacity_units_per_month: cap,
        cash_usd: dom
            .companies
            .first()
            .map(|c| c.cash_usd)
            .unwrap_or(rust_decimal::Decimal::ZERO),
        debt_usd: dom
            .companies
            .first()
            .map(|c| c.debt_usd)
            .unwrap_or(rust_decimal::Decimal::ZERO),
        share: stats.market_share,
        rd_progress: stats.rd_progress,
    };
    let cfg_ai = world.resource::<runtime::AiConfig>().0.clone();
    let mut cfg = cfg_ai.planner.clone();
    cfg.months = 3; // plan a quarter horizon
    let plan = sim_ai::plan_horizon(&st.dom, &current, &cfg_ai.weights, &cfg);
    // Convert first few decisions to strings
    let mut decisions = Vec::new();
    for d in plan.decisions.iter().take(5) {
        let s = match d.action {
            sim_ai::PlanAction::AdjustPriceFrac(df) if df < 0.0 => {
                format!("ASP{}%", (df * 100.0).round())
            }
            sim_ai::PlanAction::AdjustPriceFrac(df) if df > 0.0 => {
                format!("ASP+{}%", (df * 100.0).round())
            }
            sim_ai::PlanAction::AdjustPriceFrac(_) => "ASP±0%".into(),
            sim_ai::PlanAction::RequestCapacity(u) => format!("Capacity+{}u/mo", u),
            sim_ai::PlanAction::AllocateRndBoost(_b) => "R&D boost".into(),
            sim_ai::PlanAction::ScheduleTapeout { expedite } => {
                if expedite {
                    "Tapeout (expedite)".into()
                } else {
                    "Tapeout".into()
                }
            }
        };
        decisions.push(s);
    }
    Ok(PlanSummary {
        decisions,
        expected_score: plan.expected_score,
    })
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
struct DtoCompany {
    name: String,
    cash_cents: i64,
    debt_cents: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoSegment {
    name: String,
    base_demand_units: u64,
    price_elasticity: f32,
    base_demand_t: u64,
    ref_price_t_cents: i64,
    elasticity: f32,
    trend_pct: f32,
    sold_units: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoPricing {
    asp_cents: i64,
    unit_cost_cents: i64,
}

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
struct DtoPipeline {
    queue: Vec<DtoTapeoutReq>,
    released: Vec<core::ProductSpec>,
}

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
    campaign: Option<DtoCampaign>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SimListsDto {
    tech_nodes: Vec<String>,
    foundries: Vec<String>,
    segments: Vec<String>,
}

// -------- Campaign DTOs --------

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoGoal {
    kind: String,
    desc: String,
    progress: f32,
    deadline: String,
    done: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoCampaign {
    status: String,
    goals: Vec<DtoGoal>,
    start: String,
    end: String,
    difficulty: Option<String>,
}

// -------- Tutorial DTOs --------

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoTutStep {
    id: String,
    desc: String,
    hint: String,
    nav_page: String,
    nav_label: String,
    done: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DtoTutorial {
    active: bool,
    current_step: u8,
    steps: Vec<DtoTutStep>,
}

#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
struct TutorialCfg {
    #[serde(default)]
    cash_threshold_cents_month24: i64,
    #[serde(default)]
    steps: Vec<TutorialStepCfg>,
}

#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
struct TutorialStepCfg {
    id: String,
    desc: String,
    hint: String,
    nav: TutorialNavCfg,
}

#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
struct TutorialNavCfg {
    page: String,
    label: String,
    #[allow(dead_code)]
    button: String,
}

#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
struct CampaignScenario {
    start_date: String,
    end_date: String,
    player_start_cash_cents: i64,
    #[allow(dead_code)]
    ai_companies: usize,
    goals: Vec<YamlGoal>,
    fail_conditions: Vec<YamlFail>,
    events_yaml: String,
}

#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
#[serde(tag = "type")]
enum YamlGoal {
    #[serde(rename = "reach_share")]
    ReachShare {
        segment: String,
        min_share: f32,
        deadline: String,
    },
    #[serde(rename = "launch_node")]
    LaunchNode { node: String, deadline: String },
    #[serde(rename = "profit_target")]
    ProfitTarget { profit_cents: i64, deadline: String },
    #[serde(rename = "survive_event")]
    SurviveEvent { event_id: String, deadline: String },
}

#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
#[serde(tag = "type")]
enum YamlFail {
    #[serde(rename = "cash_below")]
    CashBelow { threshold_cents: i64 },
    #[serde(rename = "share_below")]
    ShareBelow {
        segment: String,
        min_share: f32,
        deadline: String,
    },
}

// -------- Asset schema DTOs --------
#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
#[allow(dead_code)]
struct MarketsRoot {
    segments: Vec<MarketSegSchema>,
}

#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
enum U64OrStr {
    U(u64),
    S(String),
}

#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
enum I64OrStrV {
    I(i64),
    S(String),
}

#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
#[allow(dead_code)]
struct MarketStepSchema {
    start: String,
    months: u32,
    #[serde(default)]
    base_demand_pct: Option<f32>,
    #[serde(default)]
    ref_price_pct: Option<f32>,
    #[serde(default)]
    elasticity_delta: Option<f32>,
}

#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
#[allow(dead_code)]
struct MarketSegSchema {
    id: String,
    name: String,
    base_demand_units_1990: U64OrStr,
    base_asp_cents_1990: I64OrStrV,
    elasticity: f32,
    annual_growth_pct: f32,
    #[serde(default)]
    step_events: Vec<MarketStepSchema>,
}

#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
#[allow(dead_code)]
struct TechRoot {
    nodes: Vec<TechNodeSchema>,
}

#[derive(Clone, Debug, serde::Deserialize, JsonSchema)]
#[allow(dead_code)]
struct TechNodeSchema {
    id: String,
    year_available: i32,
    wafer_cost_cents: i64,
    yield_baseline: f32,
    mask_set_cost_cents: i64,
    #[serde(default)]
    deps: Option<Vec<String>>,
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
    let trends = world.resource::<runtime::MarketTrends>().0.clone();
    let segments = dom
        .segments
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let t = trends.get(i);
            DtoSegment {
                name: s.name.clone(),
                base_demand_units: s.base_demand_units,
                price_elasticity: s.price_elasticity,
                base_demand_t: t.map(|x| x.base_demand_t).unwrap_or(s.base_demand_units),
                ref_price_t_cents: t.map(|x| x.ref_price_t_cents).unwrap_or(0),
                elasticity: t.map(|x| x.elasticity).unwrap_or(s.price_elasticity),
                trend_pct: t.map(|x| x.trend_pct).unwrap_or(0.0),
                sold_units: t.map(|x| x.sold_units).unwrap_or(0),
            }
        })
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
    let campaign = st.scenario.as_ref().map(|sc| build_campaign_dto(st, sc));
    SimStateDto {
        date,
        month_index: stats.months_run,
        companies,
        segments,
        pricing: DtoPricing {
            asp_cents,
            unit_cost_cents,
        },
        kpi,
        contracts,
        pipeline: DtoPipeline { queue, released },
        ai_plan: PlanSummary {
            decisions: vec!["n/a".into()],
            expected_score: 0.0,
        },
        config: DtoConfig {
            finance: *world.resource::<runtime::FinanceConfig>(),
            product_cost: ai_cfg.product_cost,
        },
        campaign,
    }
}

fn build_campaign_dto(st: &SimState, sc: &CampaignScenario) -> DtoCampaign {
    let world = &st.world;
    let stats = world.resource::<runtime::Stats>();
    let mut goals: Vec<DtoGoal> = Vec::new();
    // Prefer runtime campaign state if present
    if let (Some(state), Some(cfg)) = (
        world.get_resource::<runtime::CampaignStateRes>(),
        world.get_resource::<runtime::CampaignScenarioRes>(),
    ) {
        for (i, g) in cfg.goals.iter().enumerate() {
            let (desc, progress) = match g {
                runtime::GoalKind::ReachShare {
                    segment: _s,
                    min_share,
                    deadline: _,
                } => (
                    format!("Reach share ≥ {}%", (min_share * 100.0).round()),
                    (stats.market_share / (*min_share + 1e-6)).clamp(0.0, 1.0),
                ),
                runtime::GoalKind::LaunchNode { node, deadline: _ } => {
                    let pipe = world.resource::<runtime::Pipeline>();
                    let done = pipe.0.released.iter().any(|p| p.tech_node.0 == *node);
                    (
                        format!("Launch node {}", node),
                        if done { 1.0 } else { 0.0 },
                    )
                }
                runtime::GoalKind::ProfitTarget {
                    profit_cents,
                    deadline: _,
                } => {
                    let prof = persistence::decimal_to_cents_i64(stats.profit_usd).unwrap_or(0);
                    (
                        format!("Cumulative profit ≥ ${}", (*profit_cents as f64) / 100.0),
                        (prof as f32 / (*profit_cents as f32)).clamp(0.0, 1.0),
                    )
                }
                runtime::GoalKind::SurviveEvent {
                    event_id,
                    deadline: _,
                } => (format!("Survive {}", event_id), 0.0),
            };
            let st = state
                .goal_status
                .get(i)
                .cloned()
                .unwrap_or(runtime::GoalStatus::Pending);
            goals.push(DtoGoal {
                kind: "goal".into(),
                desc,
                progress,
                deadline: "".into(),
                done: matches!(st, runtime::GoalStatus::Done),
            });
        }
        let status = match state.outcome {
            runtime::CampaignOutcome::InProgress => "InProgress",
            runtime::CampaignOutcome::Success => "Success",
            runtime::CampaignOutcome::Failed => "Failed",
        }
        .to_string();
        return DtoCampaign {
            status,
            goals,
            start: sc.start_date.clone(),
            end: sc.end_date.clone(),
            difficulty: cfg.difficulty.clone(),
        };
    }
    // Fallback to simple computation from YAML
    for g in &sc.goals {
        match g {
            YamlGoal::ReachShare {
                segment: _seg,
                min_share,
                deadline,
            } => {
                let p = (stats.market_share / (*min_share + 1e-6)).clamp(0.0, 1.0);
                goals.push(DtoGoal {
                    kind: "reach_share".into(),
                    desc: format!("Reach share ≥ {}%", (min_share * 100.0).round()),
                    progress: p,
                    deadline: deadline.clone(),
                    done: p >= 1.0,
                });
            }
            YamlGoal::LaunchNode { node, deadline } => {
                let pipe = world.resource::<runtime::Pipeline>();
                let done = pipe.0.released.iter().any(|p| p.tech_node.0 == *node);
                goals.push(DtoGoal {
                    kind: "launch_node".into(),
                    desc: format!("Launch node {}", node),
                    progress: if done { 1.0 } else { 0.0 },
                    deadline: deadline.clone(),
                    done,
                });
            }
            YamlGoal::ProfitTarget {
                profit_cents,
                deadline,
            } => {
                let prof = persistence::decimal_to_cents_i64(stats.profit_usd).unwrap_or(0);
                let p = (prof as f32 / (*profit_cents as f32)).clamp(0.0, 1.0);
                goals.push(DtoGoal {
                    kind: "profit_target".into(),
                    desc: format!("Cumulative profit ≥ ${}", (*profit_cents as f64) / 100.0),
                    progress: p,
                    deadline: deadline.clone(),
                    done: p >= 1.0,
                });
            }
            YamlGoal::SurviveEvent { event_id, deadline } => {
                goals.push(DtoGoal {
                    kind: "survive_event".into(),
                    desc: format!("Survive {}", event_id),
                    progress: 0.0,
                    deadline: deadline.clone(),
                    done: false,
                });
            }
        }
    }
    DtoCampaign {
        status: "InProgress".into(),
        goals,
        start: sc.start_date.clone(),
        end: sc.end_date.clone(),
        difficulty: None,
    }
}

#[tauri::command]
fn sim_state() -> Result<SimStateDto, String> {
    // If not initialized (e.g., packaged Windows without assets path), init from embedded defaults
    if SIM_STATE.read().unwrap().is_none() {
        let _ = init_default_from_embedded();
    }
    let guard = SIM_STATE.read().unwrap();
    let st = guard.as_ref().ok_or_else(|| "sim not initialized".to_string())?;
    Ok(build_sim_state_dto(st))
}

#[tauri::command]
fn sim_lists() -> Result<SimListsDto, String> {
    let guard = SIM_STATE.read().unwrap();
    let st = guard
        .as_ref()
        .ok_or_else(|| "sim not initialized".to_string())?;
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
    Ok(SimListsDto {
        tech_nodes,
        foundries,
        segments,
    })
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ActiveModDto {
    id: String,
    kind: String,
    target: String,
    start: String,
    end: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct BalanceInfoDto {
    segments: Vec<DtoSegment>,
    active_mods: Vec<ActiveModDto>,
}

#[tauri::command]
fn sim_balance_info() -> Result<BalanceInfoDto, String> {
    let guard = SIM_STATE.read().unwrap();
    let st = guard
        .as_ref()
        .ok_or_else(|| "sim not initialized".to_string())?;
    let dto = build_sim_state_dto(st);
    // Build active mods from runtime resources: tech via ModEngine, market via MarketModEffects
    let world = &st.world;
    let mut mods_list: Vec<ActiveModDto> = Vec::new();
    if let Some(me) = world.get_non_send_resource::<runtime::ModEngineRes>() {
        for (id, start, end) in me.engine.active_effects_summary() {
            mods_list.push(ActiveModDto {
                id,
                kind: "tech".into(),
                target: "tech_tree".into(),
                start: start.to_string(),
                end: end.to_string(),
            });
        }
    }
    if let Some(mm) = world.get_resource::<runtime::MarketModEffects>() {
        for e in &mm.0 {
            mods_list.push(ActiveModDto {
                id: e.id.clone(),
                kind: "market".into(),
                target: e.segment_id.clone(),
                start: e.start.to_string(),
                end: e.end.to_string(),
            });
        }
    }
    Ok(BalanceInfoDto {
        segments: dto.segments,
        active_mods: mods_list,
    })
}

#[tauri::command]
fn sim_campaign_reset(which: Option<String>) -> Result<SimStateDto, String> {
    let id = which.unwrap_or_else(|| "1990s".to_string());
    tracing::info!(target: "ipc", which = %id, "sim_campaign_reset");
    // Resolve embedded scenario YAML
    let text = match id.as_str() {
        "1990s" => embedded::get_yaml("campaign_1990s").to_string(),
        "tutorial_24m" => embedded::get_yaml("tutorial_24m").to_string(),
        other => {
            // Back-compat: allow raw names that contain campaign/tutorial ids
            if other.contains("campaign_1990s") {
                embedded::get_yaml("campaign_1990s").to_string()
            } else if other.contains("tutorial_24m") {
                embedded::get_yaml("tutorial_24m").to_string()
            } else {
                return Err("unknown scenario".into());
            }
        }
    };
    // Validate scenario YAML
    validate_yaml::<CampaignScenario>(&text, "campaign")
        .map_err(|e| format!("campaign.yaml invalid: {e}"))?;
    let sc: CampaignScenario = serde_yaml::from_str(&text).map_err(|e| e.to_string())?;
    // Build new dom
    let start =
        chrono::NaiveDate::parse_from_str(&sc.start_date, "%Y-%m-%d").map_err(|e| e.to_string())?;
    let end =
        chrono::NaiveDate::parse_from_str(&sc.end_date, "%Y-%m-%d").map_err(|e| e.to_string())?;
    // Validate and load tech + markets assets from embedded
    let tech_text = embedded::get_yaml("tech_era_1990s");
    validate_yaml::<TechRoot>(&tech_text, "tech_era")
        .map_err(|e| format!("tech_era_1990s.yaml invalid: {e}"))?;
    let tech_nodes = load_tech_nodes_from_yaml(tech_text);
    let markets_text = embedded::get_yaml("markets_1990s");
    validate_yaml::<MarketsRoot>(&markets_text, "markets")
        .map_err(|e| format!("markets_1990s.yaml invalid: {e}"))?;
    let markets = runtime::MarketConfigRes::from_yaml_str(markets_text).unwrap_or_default();
    let segments: Vec<core::MarketSegment> = markets
        .segments
        .iter()
        .map(|s| core::MarketSegment {
            name: s.name.clone(),
            base_demand_units: s.base_demand_units_1990,
            price_elasticity: s.elasticity,
        })
        .collect();
    let dom = core::World {
        macro_state: core::MacroState {
            date: start,
            inflation_annual: 0.02,
            interest_rate: 0.05,
            fx_usd_index: 100.0,
        },
        tech_tree: tech_nodes,
        companies: vec![core::Company {
            name: "Player".into(),
            cash_usd: persistence::cents_i64_to_decimal(sc.player_start_cash_cents),
            debt_usd: rust_decimal::Decimal::ZERO,
            ip_portfolio: vec![],
        }],
        segments,
    };
    let mut world = runtime::init_world(
        dom.clone(),
        core::SimConfig {
            tick_days: 30,
            rng_seed: 42,
        },
    );
    world.insert_resource(markets);
    // Load campaign events YAML from embedded by convention
    let ev_cfg = if sc.events_yaml.contains("campaign_1990s") {
        market_events_from_yaml_str(embedded::get_yaml("events_1990s"))
    } else {
        // Fallback to embedded 1990s events
        market_events_from_yaml_str(embedded::get_yaml("events_1990s"))
    };
    world.insert_resource(ev_cfg);
    // Inject campaign scenario into runtime for goal tracking
    let mut cfg = runtime::CampaignScenarioRes {
        start,
        end,
        difficulty: None,
        goals: vec![],
        fails: vec![],
    };
    for g in &sc.goals {
        match g {
            YamlGoal::ReachShare {
                segment,
                min_share,
                deadline,
            } => {
                let d = chrono::NaiveDate::parse_from_str(deadline, "%Y-%m-%d")
                    .map_err(|e| e.to_string())?;
                cfg.goals.push(runtime::GoalKind::ReachShare {
                    segment: segment.clone(),
                    min_share: min_share.clone(),
                    deadline: d,
                });
            }
            YamlGoal::LaunchNode { node, deadline } => {
                let d = chrono::NaiveDate::parse_from_str(deadline, "%Y-%m-%d")
                    .map_err(|e| e.to_string())?;
                cfg.goals.push(runtime::GoalKind::LaunchNode {
                    node: node.clone(),
                    deadline: d,
                });
            }
            YamlGoal::ProfitTarget {
                profit_cents,
                deadline,
            } => {
                let d = chrono::NaiveDate::parse_from_str(deadline, "%Y-%m-%d")
                    .map_err(|e| e.to_string())?;
                cfg.goals.push(runtime::GoalKind::ProfitTarget {
                    profit_cents: profit_cents.clone(),
                    deadline: d,
                });
            }
            YamlGoal::SurviveEvent { event_id, deadline } => {
                let d = chrono::NaiveDate::parse_from_str(deadline, "%Y-%m-%d")
                    .map_err(|e| e.to_string())?;
                cfg.goals.push(runtime::GoalKind::SurviveEvent {
                    event_id: event_id.clone(),
                    deadline: d,
                });
            }
        }
    }
    for f in &sc.fail_conditions {
        match f {
            YamlFail::CashBelow { threshold_cents } => {
                cfg.fails.push(runtime::FailCondKind::CashBelow {
                    threshold_cents: *threshold_cents,
                })
            }
            YamlFail::ShareBelow {
                segment,
                min_share,
                deadline,
            } => {
                let d = chrono::NaiveDate::parse_from_str(deadline, "%Y-%m-%d")
                    .map_err(|e| e.to_string())?;
                cfg.fails.push(runtime::FailCondKind::ShareBelow {
                    segment: segment.clone(),
                    min_share: min_share.clone(),
                    deadline: d,
                });
            }
        }
    }
    world.insert_resource(cfg);
    world.insert_resource(runtime::CampaignStateRes::default());
    // Optional tutorial section
    let tutorial_cfg: Option<TutorialCfg> = match serde_yaml::from_str::<serde_yaml::Value>(&text) {
        Ok(v) => v
            .get("tutorial")
            .and_then(|t| serde_yaml::from_value::<TutorialCfg>(t.clone()).ok()),
        Err(_) => None,
    };
    if let Some(tcfg) = &tutorial_cfg {
        runtime::init_tutorial(&mut world, tcfg.cash_threshold_cents_month24);
    }
    // Replace global state
    {
        let mut guard = SIM_STATE.write().unwrap();
        *guard = Some(SimState {
            world,
            dom,
            busy: false,
            scenario: Some(sc),
            tutorial: tutorial_cfg,
            autosave: true,
        });
    }
    // Return the new state
    let guard = SIM_STATE.read().unwrap();
    let st = guard.as_ref().unwrap();
    let dto = build_sim_state_dto(st);
    tracing::info!(target: "ipc", date = %dto.date, "sim_campaign_reset: ok");
    Ok(dto)
}

#[tauri::command]
fn sim_override(app: tauri::AppHandle, ovr: OverrideReq) -> Result<OverrideResp, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = app.run_on_main_thread(move || {
        let mut resp = OverrideResp::default();
        let state = SIM_STATE.clone();
        let mut guard = state.write().unwrap();
        let st = match guard.as_mut() {
            Some(s) => s,
            None => {
                let _ = tx.send(Err("sim not initialized".into()));
                return;
            }
        };
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
        let _ = tx.send(Ok(resp));
    });
    rx.recv().map_err(|e| e.to_string())?
}

fn main() {
    // Initialize default world from embedded to avoid filesystem dependencies.
    let _ = init_default_from_embedded();

    tauri::Builder::<tauri::Wry>::default()
        .setup(|_app| {
            #[cfg(debug_assertions)]
            {
                let app = _app;
                // Try to open devtools on startup in debug
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.open_devtools();
                }
                // Register F12 to open devtools
                let handle = app.handle();
                let _ = app.global_shortcut().register("F12", move || {
                    if let Some(win) = handle.get_webview_window("main") {
                        let _ = win.open_devtools();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            sim_tick,
            sim_tick_quarter,
            sim_plan_quarter,
            sim_override,
            sim_state,
            sim_lists,
            sim_campaign_reset,
            sim_balance_info,
            sim_campaign_set_difficulty,
            sim_tutorial_state,
            sim_save,
            sim_list_saves,
            sim_load,
            sim_set_autosave,
            sim_export_campaign,
            sim_build_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct BuildInfo {
    version: String,
    git_sha: String,
    build_date: String,
}

#[tauri::command]
fn sim_build_info() -> Result<BuildInfo, String> {
    Ok(BuildInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_sha: option_env!("GIT_SHA").unwrap_or("unknown").to_string(),
        build_date: option_env!("BUILD_DATE").unwrap_or("").to_string(),
    })
}

#[tauri::command]
fn sim_campaign_set_difficulty(level: String) -> Result<(), String> {
    tracing::info!(target: "ipc", level = %level, "sim_campaign_set_difficulty");
    let mut g = SIM_STATE.write().unwrap();
    let st = g
        .as_mut()
        .ok_or_else(|| "sim not initialized".to_string())?;
    if let Some(mut cfg) = st.world.get_resource_mut::<runtime::CampaignScenarioRes>() {
        cfg.difficulty = Some(level.clone());
    }
    // Load presets
    #[derive(serde::Deserialize, JsonSchema)]
    struct Level {
        cash_multiplier: f32,
        min_margin_frac: f32,
        price_epsilon_frac: f32,
        take_or_pay_frac: f32,
        annual_growth_pct_multiplier: f32,
        event_severity_multiplier: f32,
    }
    #[derive(serde::Deserialize, JsonSchema)]
    struct Root {
        levels: std::collections::HashMap<String, Level>,
    }
    let text = embedded::get_yaml("difficulty").to_string();
    // Validate difficulty before applying
    validate_yaml::<Root>(&text, "difficulty")
        .map_err(|e| format!("difficulty.yaml invalid: {e}"))?;
    let root: Root = serde_yaml::from_str(&text).map_err(|e| e.to_string())?;
    let Some(preset) = root.levels.get(&level) else {
        return Err("unknown difficulty".into());
    };
    // Apply to AI config
    {
        let mut ai = st.world.resource_mut::<runtime::AiConfig>();
        ai.0.tactics.min_margin_frac = preset.min_margin_frac;
        ai.0.tactics.price_epsilon_frac = preset.price_epsilon_frac;
    }
    // Apply to difficulty params
    {
        let mut dp = st.world.resource_mut::<runtime::DifficultyParams>();
        dp.default_take_or_pay_frac = preset.take_or_pay_frac.clamp(0.0, 1.0);
    }
    // Scale markets growth
    {
        let mut markets = st.world.resource_mut::<runtime::MarketConfigRes>();
        for s in &mut markets.segments {
            s.annual_growth_pct *= preset.annual_growth_pct_multiplier;
        }
    }
    // Scale events severity for market effects in-place
    {
        if let Some(mut ev) = st.world.get_resource_mut::<runtime::MarketEventConfigRes>() {
            let mult = preset.event_severity_multiplier as f64;
            for v in &mut ev.events {
                if let Some(me) = v.get_mut("market_effect") {
                    if let Some(b) = me.get_mut("base_demand_pct") {
                        if let Some(x) = b.as_f64() {
                            *b = serde_yaml::Value::from(x * mult);
                        }
                    }
                    if let Some(e) = me.get_mut("elasticity_delta") {
                        if let Some(x) = e.as_f64() {
                            *e = serde_yaml::Value::from(x * mult);
                        }
                    }
                }
            }
        }
    }
    // Adjust player cash multiplicatively
    if let Some(c) = st.dom.companies.get_mut(0) {
        let cash = c.cash_usd;
        let m = rust_decimal::Decimal::from_f32_retain(preset.cash_multiplier as f32)
            .unwrap_or(rust_decimal::Decimal::ONE);
        c.cash_usd = cash * m;
    }
    tracing::info!(target: "ipc", "sim_campaign_set_difficulty: ok");
    Ok(())
}

fn load_tech_nodes_from_yaml(text: &str) -> Vec<core::TechNode> {
    #[derive(serde::Deserialize)]
    struct YNode {
        id: String,
        year_available: i32,
        wafer_cost_cents: i64,
        yield_baseline: f32,
        mask_set_cost_cents: i64,
        deps: Option<Vec<String>>,
    }
    #[derive(serde::Deserialize)]
    struct Root {
        nodes: Vec<YNode>,
    }
    let root: Root = serde_yaml::from_str(text).unwrap_or(Root { nodes: vec![] });
    root.nodes
        .into_iter()
        .map(|n| core::TechNode {
            id: core::TechNodeId(n.id),
            year_available: n.year_available,
            density_mtr_per_mm2: rust_decimal::Decimal::new(1, 0),
            freq_ghz_baseline: rust_decimal::Decimal::new(1, 0),
            leakage_index: rust_decimal::Decimal::new(1, 0),
            yield_baseline: rust_decimal::Decimal::from_f32_retain(n.yield_baseline)
                .unwrap_or(rust_decimal::Decimal::new(9, 1)),
            wafer_cost_usd: persistence::cents_i64_to_decimal(n.wafer_cost_cents),
            mask_set_cost_usd: persistence::cents_i64_to_decimal(n.mask_set_cost_cents),
            dependencies: n
                .deps
                .unwrap_or_default()
                .into_iter()
                .map(core::TechNodeId)
                .collect(),
        })
        .collect()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SaveInfo {
    id: i64,
    name: String,
    status: String,
    created_at: String,
    progress: u32,
}

async fn save_now(db_url: String, name: String, dom: core::World, world: runtime::World) -> Result<i64, String> {
    use persistence as p;
    let pool = p::init_db(&db_url)
        .await
        .map_err(|e| e.to_string())?;
    // Autosave flow: mark in_progress first
    let is_auto = name.starts_with("auto-");
    let sid = if is_auto {
        p::create_save_with_status(&pool, &name, None, "in_progress")
            .await
            .map_err(|e| e.to_string())?
    } else {
        p::create_save(&pool, &name, None)
            .await
            .map_err(|e| e.to_string())?
    };
    // Snapshot domain world
    let month_index = world.resource::<runtime::Stats>().months_run as i64;
    let bytes = p::serialize_world_bincode(&dom).map_err(|e| e.to_string())?;
    let _snap_id = p::insert_snapshot(&pool, sid, month_index, "bincode", &bytes)
        .await
        .map_err(|e| e.to_string())?;
    // Persist contracts
    let book = world.resource::<runtime::CapacityBook>();
    for c in &book.contracts {
        let row = p::ContractRow {
            foundry_id: c.foundry_id.clone(),
            wafers_per_month: c.wafers_per_month as i64,
            price_per_wafer_cents: c.price_per_wafer_cents,
            take_or_pay_frac: c.take_or_pay_frac,
            billing_cents_per_wafer: c.billing_cents_per_wafer,
            billing_model: c.billing_model.into(),
            lead_time_months: c.lead_time_months as i64,
            start: c.start.to_string(),
            end: c.end.to_string(),
        };
        let _ = p::insert_contract(&pool, sid, &row)
            .await
            .map_err(|e| e.to_string())?;
    }
    // Persist tapeout queue and released
    let pipe = world.resource::<runtime::Pipeline>();
    for t in &pipe.0.queue {
        let row = p::TapeoutRow {
            product_json: serde_json::to_string(&t.product).map_err(|e| e.to_string())?,
            tech_node: t.tech_node.0.clone(),
            start: t.start.to_string(),
            ready: t.ready.to_string(),
            expedite: if t.expedite { 1 } else { 0 },
            expedite_cost_cents: t.expedite_cost_cents,
        };
        let _ = p::insert_tapeout_request(&pool, sid, &row)
            .await
            .map_err(|e| e.to_string())?;
    }
    for r in &pipe.0.released {
        let row = p::ReleasedRow {
            product_json: serde_json::to_string(r).map_err(|e| e.to_string())?,
            released_at: dom.macro_state.date.to_string(),
        };
        let _ = p::insert_released_product(&pool, sid, &row)
            .await
            .map_err(|e| e.to_string())?;
    }
    // Mark done for autosave and rotate to last N=6
    if is_auto {
        let _ = p::update_save_status(&pool, sid, "done")
            .await
            .map_err(|e| e.to_string())?;
        const N: usize = 6;
        if let Ok(list) = p::list_saves_by_prefix(&pool, "auto-").await {
            if list.len() > N {
                let to_delete = list.len() - N;
                for old in list.into_iter().take(to_delete) {
                    let _ = p::delete_save(&pool, old.id).await;
                }
            }
        }
    }
    Ok(sid)
}

#[tauri::command]
async fn sim_save(app: tauri::AppHandle, name: Option<String>) -> Result<i64, String> {
    tracing::info!(target: "ipc", name = ?name, "sim_save");
    let (dom, world, nm) = {
        let g = SIM_STATE.read().unwrap();
        let st = g
            .as_ref()
            .ok_or_else(|| "sim not initialized".to_string())?;
        let date = st
            .world
            .resource::<runtime::DomainWorld>()
            .0
            .macro_state
            .date;
        let nm = name.clone().unwrap_or_else(|| {
            format!("manual-{}{:02}{:02}", date.year(), date.month(), date.day())
        });
        (st.dom.clone(), runtime::clone_world_state(&st.world), nm)
    };
    let url = saves_db_url(&app)?;
    let id = save_now(url, nm.clone(), dom, world).await?;
    tracing::info!(target: "ipc", id, "sim_save: ok");
    Ok(id)
}

#[tauri::command]
async fn sim_list_saves(app: tauri::AppHandle) -> Result<Vec<SaveInfo>, String> {
    use persistence as p;
    let url = saves_db_url(&app)?;
    let pool = p::init_db(&url)
        .await
        .map_err(|e| e.to_string())?;
    // List saves by naive query since persistence doesn't expose it
    let rows =
        sqlx::query("SELECT id, name, status, created_at FROM saves ORDER BY created_at DESC")
            .fetch_all(&pool)
            .await
            .map_err(|e| e.to_string())?;
    let mut out: Vec<SaveInfo> = Vec::new();
    for r in rows {
        let id: i64 = r.try_get("id").unwrap_or(0);
        let name: String = r.try_get("name").unwrap_or_default();
        let status: String = r.try_get("status").unwrap_or_else(|_| "done".into());
        let created_at: String = r.try_get("created_at").unwrap_or_default();
        let progress = p::latest_snapshot(&pool, id)
            .await
            .ok()
            .flatten()
            .map(|(_sid, m, _d, _f)| m as u32)
            .unwrap_or(0);
        out.push(SaveInfo {
            id,
            name,
            status,
            created_at,
            progress,
        });
    }
    Ok(out)
}

#[tauri::command]
async fn sim_load(app: tauri::AppHandle, save_id: i64) -> Result<SimStateDto, String> {
    tracing::info!(target: "ipc", save_id, "sim_load");
    use persistence as p;
    let url = saves_db_url(&app)?;
    let pool = p::init_db(&url)
        .await
        .map_err(|e| e.to_string())?;
    let (_snap_id, _m, data, _fmt) = p::latest_snapshot(&pool, save_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no snapshot".to_string())?;
    let dom = p::deserialize_world_bincode(&data).map_err(|e| e.to_string())?;
    let mut world = runtime::init_world(
        dom.clone(),
        core::SimConfig {
            tick_days: 30,
            rng_seed: 42,
        },
    );
    // Rehydrate contracts
    let contracts = p::list_contracts(&pool, save_id)
        .await
        .map_err(|e| e.to_string())?;
    if !contracts.is_empty() {
        let mut book = world.resource_mut::<runtime::CapacityBook>();
        for c in contracts {
            let start = chrono::NaiveDate::parse_from_str(&c.start, "%Y-%m-%d")
                .map_err(|e| e.to_string())?;
            let end =
                chrono::NaiveDate::parse_from_str(&c.end, "%Y-%m-%d").map_err(|e| e.to_string())?;
            book.contracts.push(runtime::FoundryContract {
                foundry_id: c.foundry_id,
                wafers_per_month: c.wafers_per_month as u32,
                price_per_wafer_cents: c.price_per_wafer_cents,
                take_or_pay_frac: c.take_or_pay_frac,
                billing_cents_per_wafer: c.billing_cents_per_wafer,
                billing_model: Box::leak(c.billing_model.into_boxed_str()),
                lead_time_months: c.lead_time_months as u8,
                start,
                end,
            });
        }
    }
    // Rehydrate released and queue
    let released = p::list_released_products(&pool, save_id)
        .await
        .map_err(|e| e.to_string())?;
    runtime::rehydrate_released_products(&mut world, &released);
    let queue = p::list_tapeout_requests(&pool, save_id)
        .await
        .map_err(|e| e.to_string())?;
    if !queue.is_empty() {
        let mut pipe = world.resource_mut::<runtime::Pipeline>();
        for t in queue {
            let product: core::ProductSpec =
                serde_json::from_str(&t.product_json).map_err(|e| e.to_string())?;
            let start = chrono::NaiveDate::parse_from_str(&t.start, "%Y-%m-%d")
                .map_err(|e| e.to_string())?;
            let ready = chrono::NaiveDate::parse_from_str(&t.ready, "%Y-%m-%d")
                .map_err(|e| e.to_string())?;
            pipe.0.queue.push(core::TapeoutRequest {
                product,
                tech_node: core::TechNodeId(t.tech_node),
                start,
                ready,
                expedite: t.expedite != 0,
                expedite_cost_cents: t.expedite_cost_cents,
            });
        }
    }
    // Replace state
    {
        let mut guard = SIM_STATE.write().unwrap();
        *guard = Some(SimState {
            world,
            dom,
            busy: false,
            scenario: None,
            tutorial: None,
            autosave: true,
        });
    }
    let g = SIM_STATE.read().unwrap();
    let st = g.as_ref().unwrap();
    let dto = build_sim_state_dto(st);
    tracing::info!(target: "ipc", date = %dto.date, "sim_load: ok");
    Ok(dto)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AutosavePolicy {
    enabled: bool,
    max_kept: usize,
}

#[tauri::command]
fn sim_set_autosave(on: bool) -> Result<AutosavePolicy, String> {
    let mut g = SIM_STATE.write().unwrap();
    let st = g
        .as_mut()
        .ok_or_else(|| "sim not initialized".to_string())?;
    st.autosave = on;
    Ok(AutosavePolicy {
        enabled: st.autosave,
        max_kept: 6,
    })
}

#[tauri::command]
fn sim_export_campaign(path: String, format: Option<String>) -> Result<(), String> {
    tracing::info!(target: "ipc", path = %path, format = ?format, "sim_export_campaign");
    // Always perform a dry-run export: clone the current world and simulate on the clone.
    let g = SIM_STATE.read().unwrap();
    let st = g
        .as_ref()
        .ok_or_else(|| "sim not initialized".to_string())?;
    // Determine months remaining until campaign end if present, else export 24 months
    let months = if let Some(cfg) = st.world.get_resource::<runtime::CampaignScenarioRes>() {
        let today = st
            .world
            .resource::<runtime::DomainWorld>()
            .0
            .macro_state
            .date;
        let end = cfg.end;
        ((end.year() - today.year()) * 12 + (end.month() as i32 - today.month() as i32)).max(0)
            as u32
    } else {
        24
    };
    if months == 0 {
        return Err("no months to simulate".into());
    }
    // Build a clone and run months in memory
    let mut dry = runtime::clone_world_state(&st.world);
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
    }
    let mut rows: Vec<Row> = Vec::with_capacity(months as usize);
    for _ in 0..months {
        let (_s, _t) = runtime::run_months_in_place(&mut dry, 1);
        let dom = dry.resource::<runtime::DomainWorld>();
        let stats = dry.resource::<runtime::Stats>();
        let pricing = dry.resource::<runtime::Pricing>();
        let date = dom.0.macro_state.date;
        rows.push(Row {
            date: date.to_string(),
            month_index: stats.months_run,
            cash_cents: persistence::decimal_to_cents_i64(dom.0.companies[0].cash_usd).unwrap_or(0),
            revenue_cents: persistence::decimal_to_cents_i64(stats.revenue_usd).unwrap_or(0),
            cogs_cents: persistence::decimal_to_cents_i64(stats.cogs_usd).unwrap_or(0),
            profit_cents: persistence::decimal_to_cents_i64(stats.profit_usd).unwrap_or(0),
            asp_cents: persistence::decimal_to_cents_i64(pricing.asp_usd).unwrap_or(0),
            unit_cost_cents: persistence::decimal_to_cents_i64(pricing.unit_cost_usd).unwrap_or(0),
            share: stats.market_share,
            output_units: stats.output_units,
            inventory_units: stats.inventory_units,
        });
    }
    if path.ends_with(".json") || format.as_deref() == Some("json") {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let s = serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?;
        std::fs::write(&path, s).map_err(|e| e.to_string())?;
        return Ok(());
    } else if path.ends_with(".parquet") || format.as_deref() == Some("parquet") {
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
        persistence::write_telemetry_parquet(&path, &trows).map_err(|e| e.to_string())?;
        return Ok(());
    }
    Err("unknown format".into())
}

#[tauri::command]
fn sim_tutorial_state() -> Result<DtoTutorial, String> {
    let g = SIM_STATE.read().unwrap();
    let st = g
        .as_ref()
        .ok_or_else(|| "sim not initialized".to_string())?;
    let world = &st.world;
    let tut = world.resource::<runtime::TutorialState>();
    let mut steps: Vec<DtoTutStep> = Vec::new();
    if let Some(cfg) = &st.tutorial {
        for s in &cfg.steps {
            let done = match s.id.as_str() {
                "price_cut" => tut.step1_price_cut_done,
                "foundry_contract" => tut.step2_contract_done,
                "tapeout_expedite" => tut.step3_tapeout_expedite_done,
                "positive_cash_24m" => tut.step4_cash_24m_done,
                _ => false,
            };
            steps.push(DtoTutStep {
                id: s.id.clone(),
                desc: s.desc.clone(),
                hint: s.hint.clone(),
                nav_page: s.nav.page.clone(),
                nav_label: s.nav.label.clone(),
                done,
            });
        }
    }
    Ok(DtoTutorial {
        active: tut.enabled,
        current_step: tut.current_step_index,
        steps,
    })
}

// ------- Helpers: events from YAML, default init, saves path

fn market_events_from_yaml_str(text: &str) -> runtime::MarketEventConfigRes {
    #[derive(serde::Deserialize)]
    struct Root {
        events: Vec<serde_yaml::Value>,
    }
    let root: Root = serde_yaml::from_str(text).unwrap_or(Root { events: vec![] });
    runtime::MarketEventConfigRes { events: root.events }
}

fn init_default_from_embedded() -> Result<(), String> {
    let date0 = chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap();
    let tech_nodes = load_tech_nodes_from_yaml(embedded::get_yaml("tech_era_1990s"));
    let markets = runtime::MarketConfigRes::from_yaml_str(embedded::get_yaml("markets_1990s"))
        .unwrap_or_default();
    let segments: Vec<core::MarketSegment> = markets
        .segments
        .iter()
        .map(|s| core::MarketSegment {
            name: s.name.clone(),
            base_demand_units: s.base_demand_units_1990,
            price_elasticity: s.elasticity,
        })
        .collect();
    let dom = core::World {
        macro_state: core::MacroState {
            date: date0,
            inflation_annual: 0.02,
            interest_rate: 0.05,
            fx_usd_index: 100.0,
        },
        tech_tree: tech_nodes,
        companies: vec![core::Company {
            name: "A".into(),
            cash_usd: rust_decimal::Decimal::new(5_000_000, 0),
            debt_usd: rust_decimal::Decimal::ZERO,
            ip_portfolio: vec![],
        }],
        segments,
    };
    let _ = core::validate_world(&dom).map_err(|e| e.to_string());
    let mut ecs = runtime::init_world(
        dom.clone(),
        core::SimConfig {
            tick_days: 30,
            rng_seed: 42,
        },
    );
    ecs.insert_resource(markets);
    ecs.insert_resource(market_events_from_yaml_str(embedded::get_yaml("events_1990s")));
    *SIM_STATE.write().unwrap() = Some(SimState {
        world: ecs,
        dom,
        busy: false,
        scenario: None,
        tutorial: None,
        autosave: true,
    });
    Ok(())
}

fn saves_db_url(app: &tauri::AppHandle) -> Result<String, String> {
    use tauri::path::BaseDirectory;
    let p = app
        .path()
        .resolve("chip-tycoon/saves/main.db", BaseDirectory::AppData)
        .map_err(|e| e.to_string())?;
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut s = p.to_string_lossy().to_string();
    // Normalize path separators for sqlite URL on Windows
    if cfg!(windows) {
        s = s.replace('\\', "/");
    }
    Ok(format!("sqlite://{}", s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_ticks_increase_month_index() {
        // Initialize state
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
                cash_usd: rust_decimal::Decimal::new(1_000_000, 0),
                debt_usd: rust_decimal::Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.2,
            }],
        };
        let ecs = runtime::init_world(
            dom.clone(),
            core::SimConfig {
                tick_days: 30,
                rng_seed: 42,
            },
        );
        *SIM_STATE.write().unwrap() = Some(SimState {
            world: ecs,
            dom,
            busy: false,
            scenario: None,
            tutorial: None,
            autosave: true,
        });
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
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: rust_decimal::Decimal::new(1_000_000, 0),
                debt_usd: rust_decimal::Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.2,
            }],
        };
        let ecs = runtime::init_world(
            dom.clone(),
            core::SimConfig {
                tick_days: 30,
                rng_seed: 42,
            },
        );
        *SIM_STATE.write().unwrap() = Some(SimState {
            world: ecs,
            dom,
            busy: true,
            scenario: None,
            tutorial: None,
            autosave: true,
        });
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
            macro_state: core::MacroState {
                date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
                inflation_annual: 0.02,
                interest_rate: 0.05,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![core::TechNode {
                id: core::TechNodeId("N90".into()),
                year_available: 1990,
                density_mtr_per_mm2: rust_decimal::Decimal::new(1, 0),
                freq_ghz_baseline: rust_decimal::Decimal::new(1, 0),
                leakage_index: rust_decimal::Decimal::new(1, 0),
                yield_baseline: rust_decimal::Decimal::new(9, 1),
                wafer_cost_usd: rust_decimal::Decimal::new(1000, 0),
                mask_set_cost_usd: rust_decimal::Decimal::new(5000, 0),
                dependencies: vec![],
            }],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: rust_decimal::Decimal::new(1_000_000, 0),
                debt_usd: rust_decimal::Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.2,
            }],
        };
        let ecs = runtime::init_world(
            dom.clone(),
            core::SimConfig {
                tick_days: 30,
                rng_seed: 42,
            },
        );
        *SIM_STATE.write().unwrap() = Some(SimState {
            world: ecs,
            dom,
            busy: false,
            scenario: None,
            tutorial: None,
            autosave: true,
        });

        // Apply price +5%
        let r = sim_override(OverrideReq {
            price_delta_frac: Some(0.05),
            rd_delta_cents: None,
            capacity_request: None,
            tapeout: None,
        })
        .expect("override");
        assert!(r.asp_cents.unwrap_or(0) > 0);

        // Apply R&D budget increase
        let _ = sim_override(OverrideReq {
            price_delta_frac: None,
            rd_delta_cents: Some(10_000),
            capacity_request: None,
            tapeout: None,
        })
        .expect("rd");
        {
            let g = SIM_STATE.read().unwrap();
            let world = &g.as_ref().unwrap().world;
            let b = world.resource::<runtime::RnDBudgetCents>().0;
            assert_eq!(b, 10_000);
        }

        // Capacity request
        let _ = sim_override(OverrideReq {
            price_delta_frac: None,
            rd_delta_cents: None,
            capacity_request: Some(CapacityReq {
                wafers_per_month: 1000,
                months: 12,
                billing_cents_per_wafer: Some(5000),
                take_or_pay_frac: Some(1.0),
            }),
            tapeout: None,
        })
        .expect("cap");
        {
            let g = SIM_STATE.read().unwrap();
            let world = &g.as_ref().unwrap().world;
            assert!(!world
                .resource::<runtime::CapacityBook>()
                .contracts
                .is_empty());
        }

        // Tapeout expedited, then tick to ready and expect release
        let resp = sim_override(OverrideReq {
            price_delta_frac: None,
            rd_delta_cents: None,
            capacity_request: None,
            tapeout: Some(TapeoutReq {
                perf_index: 0.8,
                die_area_mm2: 100.0,
                tech_node: "N90".into(),
                expedite: Some(true),
            }),
        })
        .expect("tapeout");
        let ready =
            chrono::NaiveDate::parse_from_str(&resp.tapeout_ready.unwrap(), "%Y-%m-%d").unwrap();
        // Compute months to ready from current date
        let start = SIM_STATE
            .read()
            .unwrap()
            .as_ref()
            .unwrap()
            .dom
            .macro_state
            .date;
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
    fn export_campaign_is_dry_run_and_file_valid() {
        // Reset campaign to a known state
        let _ = sim_campaign_reset(Some("1990s".into())).expect("reset");
        // Capture KPI hash before
        let s1 = sim_state().expect("state before");
        let kpi_before = serde_json::to_string(&s1.kpi).expect("ser kpi");
        // Export to JSON in telemetry dir
        let path = "telemetry/test_export_campaign.json".to_string();
        let _ = std::fs::remove_file(&path);
        sim_export_campaign(path.clone(), Some("json".into())).expect("export");
        // State unchanged
        let s2 = sim_state().expect("state after");
        let kpi_after = serde_json::to_string(&s2.kpi).expect("ser kpi2");
        assert_eq!(kpi_after, kpi_before, "KPI changed after dry-run export");
        assert_eq!(
            s2.month_index, s1.month_index,
            "month index changed after export"
        );
        // File valid JSON of array of rows
        let text = std::fs::read_to_string(&path).expect("read export");
        #[derive(serde::Deserialize)]
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
        }
        let rows: Vec<Row> = serde_json::from_str(&text).expect("parse json");
        assert!(!rows.is_empty(), "no rows exported");
    }

    #[test]
    fn state_dto_roundtrip_and_updates() {
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
                density_mtr_per_mm2: rust_decimal::Decimal::new(1, 0),
                freq_ghz_baseline: rust_decimal::Decimal::new(1, 0),
                leakage_index: rust_decimal::Decimal::new(1, 0),
                yield_baseline: rust_decimal::Decimal::new(9, 1),
                wafer_cost_usd: rust_decimal::Decimal::new(1000, 0),
                mask_set_cost_usd: rust_decimal::Decimal::new(5000, 0),
                dependencies: vec![],
            }],
            companies: vec![core::Company {
                name: "A".into(),
                cash_usd: rust_decimal::Decimal::new(1_000_000, 0),
                debt_usd: rust_decimal::Decimal::ZERO,
                ip_portfolio: vec![],
            }],
            segments: vec![core::MarketSegment {
                name: "Seg".into(),
                base_demand_units: 1_000_000,
                price_elasticity: -1.2,
            }],
        };
        let ecs = runtime::init_world(
            dom.clone(),
            core::SimConfig {
                tick_days: 30,
                rng_seed: 42,
            },
        );
        *SIM_STATE.write().unwrap() = Some(SimState {
            world: ecs,
            dom,
            busy: false,
            scenario: None,
            tutorial: None,
            autosave: true,
        });
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
        let _ = sim_override(OverrideReq {
            price_delta_frac: Some(0.05),
            rd_delta_cents: None,
            capacity_request: None,
            tapeout: None,
        })
        .unwrap();
        let s3 = sim_state().unwrap();
        assert!(s3.pricing.asp_cents >= s2.pricing.asp_cents);
    }

    #[test]
    fn autosave_transaction_and_rotation() {
        // Clean DB to start fresh
        let _ = std::fs::remove_file("./saves/main.db");
        let _ = std::fs::remove_dir_all("./saves");
        // Reset campaign and ensure autosave ON
        let _ = sim_campaign_reset(Some("1990s".into())).expect("reset");
        let _ = sim_set_autosave(true).expect("enable autosave");
        // Run two quarters and wait for autosaves
        let rt = tauri::async_runtime::TokioRuntime::new().expect("rt");
        let _ = rt.block_on(sim_tick_quarter()).expect("q1");
        let _ = rt.block_on(sim_tick_quarter()).expect("q2");
        // Poll for autosaves to appear
        let mut tries = 0;
        loop {
            let list = rt.block_on(sim_list_saves()).unwrap_or_default();
            let autos: Vec<_> = list
                .into_iter()
                .filter(|s| s.name.starts_with("auto-"))
                .collect();
            if autos.len() >= 2 || tries > 50 {
                assert!(autos.len() >= 2, "expected at least 2 autosaves");
                assert!(
                    autos.iter().all(|s| s.status == "done"),
                    "autosaves not done"
                );
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            tries += 1;
        }
        // Now run 6 more quarters and check rotation keeps only last 6
        for _ in 0..6 {
            let _ = rt.block_on(sim_tick_quarter()).expect("quarter");
        }
        // Wait for rotation to settle
        let mut tries = 0;
        loop {
            let list = rt.block_on(sim_list_saves()).unwrap_or_default();
            let autos: Vec<_> = list
                .into_iter()
                .filter(|s| s.name.starts_with("auto-"))
                .collect();
            if autos.len() == 6 || tries > 60 {
                assert_eq!(autos.len(), 6, "rotation should keep last 6 autosaves");
                assert!(
                    autos.iter().all(|s| s.status == "done"),
                    "rotated autosaves should be done"
                );
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            tries += 1;
        }
    }

    #[test]
    fn difficulty_presets_apply() {
        // Reset campaign to ensure markets/events loaded
        let _ = sim_campaign_reset(Some("1990s".into())).expect("reset");
        // Capture baseline values
        let (base_min_margin, base_growth, base_event): (f32, f32, f64) = {
            let g = SIM_STATE.read().unwrap();
            let st = g.as_ref().unwrap();
            let ai = st.world.resource::<runtime::AiConfig>().0.clone();
            let markets = st.world.resource::<runtime::MarketConfigRes>().clone();
            // take first segment growth as baseline
            let growth = markets
                .segments
                .first()
                .map(|s| s.annual_growth_pct)
                .unwrap_or(1.0);
            // pick a market event base_demand_pct if any
            let mut ev_mag = 0.0f64;
            if let Some(ev) = st.world.get_resource::<runtime::MarketEventConfigRes>() {
                for v in &ev.0.events {
                    if let Some(me) = v.get("market_effect") {
                        if let Some(b) = me.get("base_demand_pct").and_then(|x| x.as_f64()) {
                            ev_mag = b;
                            break;
                        }
                    }
                }
            }
            (ai.tactics.min_margin_frac, growth, ev_mag)
        };
        // Apply hard difficulty
        sim_campaign_set_difficulty("hard".into()).expect("apply hard");
        // Check updated values
        let g = SIM_STATE.read().unwrap();
        let st = g.as_ref().unwrap();
        let ai2 = st.world.resource::<runtime::AiConfig>().0.clone();
        assert!(ai2.tactics.min_margin_frac >= 0.10 - 1e-6);
        let markets2 = st.world.resource::<runtime::MarketConfigRes>().clone();
        let growth2 = markets2
            .segments
            .first()
            .map(|s| s.annual_growth_pct)
            .unwrap_or(1.0);
        assert!(growth2 <= base_growth * 0.81 + 1e-6);
        if base_event > 0.0 {
            // event magnitude increased by ~1.25x
            let mut ev_mag2 = 0.0f64;
            if let Some(ev) = st.world.get_resource::<runtime::MarketEventConfigRes>() {
                for v in &ev.0.events {
                    if let Some(me) = v.get("market_effect") {
                        if let Some(b) = me.get("base_demand_pct").and_then(|x| x.as_f64()) {
                            ev_mag2 = b;
                            break;
                        }
                    }
                }
            }
            assert!(ev_mag2 >= base_event * 1.24);
        }
    }

    #[test]
    fn yaml_schema_validation_works() {
        // Valid markets
        let markets = std::fs::read_to_string("assets/data/markets_1990s.yaml").expect("read");
        assert!(validate_yaml::<MarketsRoot>(&markets, "markets").is_ok());
        // Broken markets (id not string)
        let broken = "segments: [ { id: 123, name: A, base_demand_units_1990: 1, base_asp_cents_1990: 1, elasticity: -1.0, annual_growth_pct: 0.0 } ]";
        let err = validate_yaml::<MarketsRoot>(broken, "markets").unwrap_err();
        assert!(err.contains("/segments/0/id"), "err: {}", err);
        // Valid tech era
        let tech = std::fs::read_to_string("assets/data/tech_era_1990s.yaml").expect("read");
        assert!(validate_yaml::<TechRoot>(&tech, "tech_era").is_ok());
        // Broken tech (year_available wrong type)
        let broken_t = "nodes: [ { id: N90, year_available: foo, wafer_cost_cents: 1, yield_baseline: 0.9, mask_set_cost_cents: 1 } ]";
        let err2 = validate_yaml::<TechRoot>(broken_t, "tech_era").unwrap_err();
        assert!(err2.contains("/nodes/0/year_available"), "err: {}", err2);
        // Valid campaign
        let camp = std::fs::read_to_string("assets/scenarios/campaign_1990s.yaml").expect("read");
        assert!(validate_yaml::<CampaignScenario>(&camp, "campaign").is_ok());
        // Broken campaign (missing start_date)
        let broken_c = "end_date: 2000-01-01";
        let err3 = validate_yaml::<CampaignScenario>(broken_c, "campaign").unwrap_err();
        assert!(err3.contains("/start_date"), "err: {}", err3);
        // Valid difficulty
        let diff = std::fs::read_to_string("assets/scenarios/difficulty.yaml").expect("read");
        #[derive(serde::Deserialize, JsonSchema)]
        struct Level {
            cash_multiplier: f32,
            min_margin_frac: f32,
            price_epsilon_frac: f32,
            take_or_pay_frac: f32,
            annual_growth_pct_multiplier: f32,
            event_severity_multiplier: f32,
        }
        #[derive(serde::Deserialize, JsonSchema)]
        struct Root {
            levels: std::collections::HashMap<String, Level>,
        }
        assert!(validate_yaml::<Root>(&diff, "difficulty").is_ok());
        let broken_d = "levels: { easy: { min_margin_frac: low } }";
        let err4 = validate_yaml::<Root>(broken_d, "difficulty").unwrap_err();
        assert!(
            err4.contains("/levels/easy/min_margin_frac"),
            "err: {}",
            err4
        );
    }
}
