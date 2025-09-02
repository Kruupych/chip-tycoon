import { invoke } from "@tauri-apps/api/core";

export type SimSnapshot = {
  months_run: number;
  revenue_usd: string;
  profit_usd: string;
  contract_costs_cents: number;
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

export async function simOverride(payload: {
  pricing?: number;
  rdDelta?: number;
  capacityRequest?: { wafersPerMonth: number; months: number };
}) {
  return invoke<string>("sim_override", { ovr: payload });
}

