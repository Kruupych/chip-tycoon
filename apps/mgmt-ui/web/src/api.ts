import { invoke } from "@tauri-apps/api/core";

export type SimSnapshot = {
  months_run: number;
  cash_cents: number;
  revenue_cents: number;
  cogs_cents: number;
  profit_cents: number;
  contract_costs_cents: number;
  asp_cents: number;
  unit_cost_cents: number;
  market_share: number;
  rd_progress: number;
  output_units: number;
  defect_units: number;
  inventory_units: number;
};

export type PlanSummary = { decisions: string[]; expected_score: number };

export async function simTick(months: number) {
  return invoke<SimSnapshot>("sim_tick", { months });
}

export async function simPlanQuarter() {
  return invoke<PlanSummary>("sim_plan_quarter");
}

export type OverrideReq = {
  price_delta_frac?: number;
  rd_delta_cents?: number;
  capacity_request?: {
    wafers_per_month: number;
    months: number;
    billing_cents_per_wafer?: number;
    take_or_pay_frac?: number;
  };
  tapeout?: {
    perf_index: number;
    die_area_mm2: number;
    tech_node: string;
    expedite?: boolean;
  };
};

export type OverrideResp = {
  asp_cents?: number;
  rd_budget_cents?: number;
  capacity_summary?: string;
  tapeout_ready?: string;
};

export async function simOverride(payload: OverrideReq) {
  return invoke<OverrideResp>("sim_override", { ovr: payload });
}

export type SimStateDto = {
  date: string;
  month_index: number;
  companies: { name: string; cash_cents: number; debt_cents: number }[];
  segments: { name: string; base_demand_units: number; price_elasticity: number; base_demand_t: number; ref_price_t_cents: number; elasticity: number; trend_pct: number; sold_units: number }[];
  pricing: { asp_cents: number; unit_cost_cents: number };
  kpi: {
    cash_cents: number;
    revenue_cents: number;
    cogs_cents: number;
    contract_costs_cents: number;
    profit_cents: number;
    share: number;
    rd_pct: number;
    output_units: number;
    inventory_units: number;
  };
  contracts: {
    foundry_id: string;
    wafers_per_month: number;
    billing_cents_per_wafer: number;
    take_or_pay_frac: number;
    start: string;
    end: string;
  }[];
  pipeline: {
    queue: {
      tech_node: string;
      start: string;
      ready: string;
      expedite: boolean;
      expedite_cost_cents: number;
      perf_index: number;
    }[];
    released: { tech_node: { 0: string } }[] | any[];
  };
};

export type SimListsDto = {
  tech_nodes: string[];
  foundries: string[];
  segments: string[];
};

export async function getSimState() {
  return invoke<SimStateDto>("sim_state");
}

export async function getSimLists() {
  return invoke<SimListsDto>("sim_lists");
}

export type CampaignDto = { status: string; goals: { kind: string; desc: string; progress: number; deadline: string; done: boolean }[]; start: string; end: string };
export async function simCampaignReset(which?: string) {
  return invoke("sim_campaign_reset", { which });
}
export type BalanceInfo = { segments: SimStateDto["segments"]; active_mods: { id: string; kind: string; target: string; start: string; end: string }[] };
export async function simBalanceInfo() {
  return invoke<BalanceInfo>("sim_balance_info");
}
export async function simCampaignSetDifficulty(level: string) {
  return invoke("sim_campaign_set_difficulty", { level });
}
