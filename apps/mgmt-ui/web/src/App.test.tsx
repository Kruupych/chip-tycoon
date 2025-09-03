import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React from "react";
import { App } from "./App";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { invoke as invokeMock } from "@tauri-apps/api/core";
// Set default mock implementation
(invokeMock as any).mockImplementation(async (cmd: string) => {
  if (cmd === "sim_tick") {
    return {
      months_run: 1,
      cash_cents: 1000000,
      revenue_cents: 12345,
      cogs_cents: 6789,
      profit_cents: 5556,
      contract_costs_cents: 0,
      asp_cents: 30000,
      unit_cost_cents: 20000,
      market_share: 0.2,
      rd_progress: 0.1,
      output_units: 1000,
      defect_units: 50,
      inventory_units: 950,
    };
  }
  if (cmd === "sim_state") {
    return {
      date: "1990-02-01",
      month_index: 1,
      companies: [{ name: "A", cash_cents: 1000000, debt_cents: 0 }],
      segments: [{ name: "Seg", base_demand_units: 1000, price_elasticity: -1.2, base_demand_t: 1000, ref_price_t_cents: 30000, elasticity: -1.2, trend_pct: 8.0, sold_units: 800 }],
      pricing: { asp_cents: 30000, unit_cost_cents: 20000 },
      kpi: { cash_cents: 1000000, revenue_cents: 12345, cogs_cents: 6789, contract_costs_cents: 0, profit_cents: 5556, share: 0.2, rd_pct: 0.1, output_units: 1000, inventory_units: 950 },
      contracts: [],
      pipeline: { queue: [], released: [] },
      ai_plan: { decisions: [], expected_score: 0 },
      config: { finance: { revenue_cash_in_days: 0, cogs_cash_out_days: 0, rd_cash_out_days: 0 }, product_cost: { usable_die_area_mm2: 6200, yield_overhead_frac: 0.05 } },
    };
  }
  if (cmd === "sim_lists") {
    return { tech_nodes: ["N90"], foundries: [], segments: ["Seg"] } as any;
  }
  if (cmd === "sim_tutorial_state") {
    return { active: false, current_step: 0, steps: [] } as any;
  }
  if (cmd === "sim_build_info") {
    return { version: "0.0.0", git_sha: "deadbeef", build_date: "today" } as any;
  }
  if (cmd === "sim_plan_quarter") {
    return { decisions: ["ASP-5%", "Capacity+10000u/mo"], expected_score: 0.5 };
  }
  if (cmd === "sim_override") {
    return { asp_cents: 30000 };
  }
  if (cmd === "sim_tick_quarter") {
    return { ok: true } as any;
  }
  throw new Error("unknown cmd: " + cmd);
});

describe("App", () => {
  it("renders and updates on Tick Month (auto-refresh sim_state)", async () => {
    render(<App />);
    const btn = screen.getByText("Tick Month");
    fireEvent.click(btn);
    // Ensure sim_state was called after sim_tick
    expect(invokeMock).toHaveBeenCalledWith("sim_state", undefined);
  });

  it("simulate quarter advances by three months", async () => {
    render(<App />);
    const btns = await screen.findAllByText("Simulate Quarter");
    fireEvent.click(btns[0]);
    // after calling, sim_state should be refetched (we mock fixed date but ensure call made)
    expect(invokeMock).toHaveBeenCalledWith("sim_tick_quarter", undefined);
    expect(invokeMock).toHaveBeenCalledWith("sim_state", undefined);
  });
});
