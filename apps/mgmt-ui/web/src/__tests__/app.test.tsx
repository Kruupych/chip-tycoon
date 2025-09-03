import { describe, expect, it, vi } from "vitest";
import React from "react";
import { render, screen } from "@testing-library/react";
import { App } from "../App";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { invoke as invokeMock } from "@tauri-apps/api/core";
// Provide minimal stubs for required queries
(invokeMock as any).mockImplementation(async (cmd: string) => {
  if (cmd === "sim_lists") return { tech_nodes: ["N90"], foundries: [], segments: ["Seg"] };
  if (cmd === "sim_state") return {
    date: "1990-01-01",
    month_index: 0,
    companies: [{ name: "A", cash_cents: 0, debt_cents: 0 }],
    segments: [{ name: "Seg", base_demand_units: 0, price_elasticity: -1.0, base_demand_t: 0, ref_price_t_cents: 0, elasticity: -1.0, trend_pct: 0, sold_units: 0 }],
    pricing: { asp_cents: 0, unit_cost_cents: 0 },
    kpi: { cash_cents: 0, revenue_cents: 0, cogs_cents: 0, contract_costs_cents: 0, profit_cents: 0, share: 0, rd_pct: 0, output_units: 0, inventory_units: 0 },
    contracts: [], pipeline: { queue: [], released: [] }, ai_plan: { decisions: [], expected_score: 0 }, config: { finance: {}, product_cost: { usable_die_area_mm2: 0, yield_overhead_frac: 0 } }
  };
  if (cmd === "sim_tutorial_state") return { active: false, current_step: 0, steps: [] };
  if (cmd === "sim_build_info") return { version: "0.0.0", git_sha: "", build_date: "" };
  return {} as any;
});

describe("App", () => {
  it("renders heading and buttons", () => {
    render(<App />);
    expect(screen.getByText(/Mgmt/)).toBeTruthy();
    expect(screen.getByText(/Tick Month/)).toBeTruthy();
    expect(screen.getByText(/AI Plan/)).toBeTruthy();
  });
});
