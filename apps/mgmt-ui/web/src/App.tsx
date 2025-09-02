import React from "react";
import { useAppStore } from "./store";
import { simTick, simPlanQuarter } from "./api";

export function App() {
  const { snapshot, setSnapshot, loading, setLoading } = useAppStore();
  return (
    <div style={{ padding: 16, fontFamily: "sans-serif" }}>
      <h1>Chip Tycoon Mgmt UI</h1>
      <button
        disabled={loading}
        onClick={async () => {
          setLoading(true);
          try {
            const snap = await simTick(1);
            setSnapshot(snap);
          } finally {
            setLoading(false);
          }
        }}
      >
        Tick Month
      </button>
      <button
        disabled={loading}
        onClick={async () => {
          const plan = await simPlanQuarter();
          alert(`Plan: ${plan.decisions.join(", ")}`);
        }}
      >
        AI Plan
      </button>
      <div style={{ marginTop: 16 }}>
        {snapshot ? (
          <div>
            <div>Months: {snapshot.months_run}</div>
            <div>Revenue: ${snapshot.revenue_usd}</div>
            <div>Profit: ${snapshot.profit_usd}</div>
            <div>Contract Costs: {snapshot.contract_costs_cents}c</div>
            <div>Share: {(snapshot.market_share * 100).toFixed(1)}%</div>
            <div>Output: {snapshot.output_units}</div>
            <div>Inventory: {snapshot.inventory_units}</div>
          </div>
        ) : (
          <div>No data yet.</div>
        )}
      </div>
    </div>
  );
}

