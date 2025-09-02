import { describe, it, expect, vi, beforeAll } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React from "react";
import { App } from "./App";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
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
    if (cmd === "sim_plan_quarter") {
      return { decisions: ["AdjustPrice(-5%)"], expected_score: 0.5 };
    }
    if (cmd === "sim_override") {
      return { asp_cents: 30000 };
    }
    throw new Error("unknown cmd");
  }),
}));

describe("App", () => {
  it("renders and updates on Tick Month", async () => {
    render(<App />);
    const btn = screen.getByText("Tick Month");
    fireEvent.click(btn);
    // After tick, dashboard should show month index
    const month = await screen.findByText(/Month #1/);
    expect(month).toBeDefined();
  });
});

