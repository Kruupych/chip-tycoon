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
