import { invoke } from "@tauri-apps/api/core";

// Unified, safe IPC invoker with robust error reporting
export async function invokeSafe<T>(cmd: string, payload?: Record<string, any>): Promise<T> {
  try {
    // Support optional global injection for tests/manual runs
    const anyWin = (window as any);
    const inv: typeof invoke = anyWin?.__tauriInvoke ?? invoke;
    return (await inv(cmd, payload)) as T;
  } catch (err: any) {
    const msg = typeof err === "string" ? err : err?.message ?? JSON.stringify(err);
    // Always log details for devtools
    console.error({ cmd, payload, error: err });
    // Normalize to Error for UI
    throw new Error(`IPC ${cmd} failed: ${msg}`);
  }
}

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
  return invokeSafe<SimSnapshot>("sim_tick", { months });
}

export async function simPlanQuarter() {
  return invokeSafe<PlanSummary>("sim_plan_quarter");
}

export async function simTickQuarter() {
  return invokeSafe<SimSnapshot>("sim_tick_quarter");
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
  return invokeSafe<OverrideResp>("sim_override", { ovr: payload });
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
  ai_plan: PlanSummary;
  config: { finance: any; product_cost: { usable_die_area_mm2: number; yield_overhead_frac: number } };
  campaign?: { status: string; goals: { kind: string; desc: string; progress: number; deadline: string; done: boolean }[]; start: string; end: string; difficulty?: string } | null;
};

export type SimListsDto = {
  tech_nodes: string[];
  foundries: string[];
  segments: string[];
};

export async function getSimState() {
  return invokeSafe<SimStateDto>("sim_state");
}

export async function getSimLists() {
  return invokeSafe<SimListsDto>("sim_lists");
}

export type CampaignDto = { status: string; goals: { kind: string; desc: string; progress: number; deadline: string; done: boolean }[]; start: string; end: string };
export async function simCampaignReset(which?: string) {
  return invokeSafe("sim_campaign_reset", { which });
}
export type BalanceInfo = { segments: SimStateDto["segments"]; active_mods: { id: string; kind: string; target: string; start: string; end: string }[] };
export async function simBalanceInfo() {
  return invokeSafe<BalanceInfo>("sim_balance_info");
}
export async function simCampaignSetDifficulty(level: string) {
  return invokeSafe("sim_campaign_set_difficulty", { level });
}

export type TutorialDto = {
  active: boolean;
  current_step: number;
  steps: { id: string; desc: string; hint: string; nav_page: string; nav_label: string; done: boolean }[];
};
export async function simTutorialState() {
  return invokeSafe<TutorialDto>("sim_tutorial_state");
}

export type BuildInfo = { version: string; git_sha: string; build_date: string };
export async function simBuildInfo() {
  return invokeSafe<BuildInfo>("sim_build_info");
}

// Save/Load and export helpers
export async function simSave(name?: string) {
  return invokeSafe<number>("sim_save", { name });
}

export type SaveInfo = { id: number; name: string; status?: string; created_at: string; progress: number };
export async function simListSaves() {
  return invokeSafe<SaveInfo[]>("sim_list_saves");
}

export async function simLoad(save_id: number) {
  // Note param name is snake_case per backend signature
  return invokeSafe<SimStateDto>("sim_load", { save_id });
}

export async function simSetAutosave(on: boolean) {
  return invokeSafe<{ enabled: boolean; max_kept: number }>("sim_set_autosave", { on });
}

export async function simExportCampaign(path: string, format?: "json" | "parquet") {
  return invokeSafe<void>("sim_export_campaign", { path, format });
}
