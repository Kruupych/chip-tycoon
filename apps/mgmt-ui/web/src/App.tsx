import React, { useState } from "react";
import { useAppStore } from "./store";
import { simTick, simPlanQuarter, simOverride } from "./api";

export function App() {
  const { snapshot, setSnapshot, loading, setLoading } = useAppStore();
  const [nav, setNav] = useState<"dashboard" | "markets" | "rd" | "capacity" | "ai">(
    "dashboard"
  );
  return (
    <div style={{ display: "flex", height: "100vh", fontFamily: "sans-serif" }}>
      <div style={{ width: 220, borderRight: "1px solid #ddd", padding: 12 }}>
        <h3>Mgmt</h3>
        <div>
          {[
            ["dashboard", "Dashboard"],
            ["markets", "Markets"],
            ["rd", "R&D / Tapeout"],
            ["capacity", "Capacity"],
            ["ai", "AI Plan"],
          ].map(([k, label]) => (
            <div key={k}>
              <button
                onClick={() => setNav(k as any)}
                style={{
                  background: nav === k ? "#eee" : "transparent",
                  border: "none",
                  padding: 6,
                  cursor: "pointer",
                }}
              >
                {label}
              </button>
            </div>
          ))}
        </div>
        <div style={{ marginTop: 16 }}>
          <button
            disabled={loading}
            onClick={async () => {
              setLoading(true);
              try {
                const snap = await simTick(1);
                setSnapshot(snap);
              } catch (e: any) {
                alert(e?.toString?.() ?? String(e));
              } finally {
                setLoading(false);
              }
            }}
          >
            Tick Month
          </button>
        </div>
      </div>
      <div style={{ flex: 1, padding: 16 }}>
        <div style={{ marginBottom: 8, color: "#666" }}>
          Month #{snapshot?.months_run ?? 0}
        </div>
        {nav === "dashboard" && <Dashboard />}
        {nav === "markets" && <Markets />}
        {nav === "rd" && <RD />}
        {nav === "capacity" && <Capacity />}
        {nav === "ai" && <AIPlan />}
      </div>
    </div>
  );
}

function cents(n?: number) {
  return `$${((n ?? 0) / 100).toFixed(2)}`;
}

function Dashboard() {
  const { snapshot } = useAppStore();
  if (!snapshot) return <div>No data yet.</div>;
  return (
    <div>
      <h2>Dashboard</h2>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 12 }}>
        <Kpi label="Cash" value={cents(snapshot.cash_cents)} />
        <Kpi label="Revenue" value={cents(snapshot.revenue_cents)} />
        <Kpi label="COGS" value={cents(snapshot.cogs_cents)} />
        <Kpi label="Contract Costs" value={cents(snapshot.contract_costs_cents)} />
        <Kpi label="Profit" value={cents(snapshot.profit_cents)} />
        <Kpi label="ASP" value={cents(snapshot.asp_cents)} />
        <Kpi label="Unit Cost" value={cents(snapshot.unit_cost_cents)} />
        <Kpi label="Share" value={`${(snapshot.market_share * 100).toFixed(1)}%`} />
        <Kpi label="R&D" value={`${(snapshot.rd_progress * 100).toFixed(1)}%`} />
        <Kpi label="Output" value={`${snapshot.output_units}`} />
        <Kpi label="Inventory" value={`${snapshot.inventory_units}`} />
      </div>
    </div>
  );
}

function Markets() {
  const [delta, setDelta] = useState(0);
  return (
    <div>
      <h2>Markets</h2>
      <div>
        <label>Price delta (%): </label>
        <input
          type="number"
          value={delta}
          onChange={(e) => setDelta(Number(e.target.value))}
          style={{ width: 100 }}
        />
        <button
          onClick={async () => {
            const resp = await simOverride({ price_delta_frac: delta / 100 });
            alert(`New ASP: ${cents(resp.asp_cents)}`);
          }}
        >
          Apply
        </button>
      </div>
    </div>
  );
}

function RD() {
  const [rd, setRd] = useState(0);
  const [expedite, setExpedite] = useState(false);
  const [tech, setTech] = useState("N90");
  return (
    <div>
      <h2>R&D / Tapeout</h2>
      <div>
        <label>R&D Δ (cents/mo): </label>
        <input type="number" value={rd} onChange={(e) => setRd(Number(e.target.value))} />
        <button onClick={async () => {
          const resp = await simOverride({ rd_delta_cents: rd });
          alert(`New RD budget: ${resp.rd_budget_cents}c/mo`);
        }}>Apply</button>
      </div>
      <div style={{ marginTop: 12 }}>
        <label>Tech node: </label>
        <input value={tech} onChange={(e) => setTech(e.target.value)} />
        <label> Expedite </label>
        <input type="checkbox" checked={expedite} onChange={(e) => setExpedite(e.target.checked)} />
        <button onClick={async () => {
          const resp = await simOverride({ tapeout: { perf_index: 0.8, die_area_mm2: 100, tech_node: tech, expedite } });
          alert(`Tapeout ready: ${resp.tapeout_ready}`);
        }}>Queue Tapeout</button>
      </div>
    </div>
  );
}

function Capacity() {
  const [wpm, setWpm] = useState(1000);
  const [months, setMonths] = useState(12);
  return (
    <div>
      <h2>Capacity</h2>
      <div>
        <label>Wafers/mo: </label>
        <input type="number" value={wpm} onChange={(e) => setWpm(Number(e.target.value))} />
        <label> Months: </label>
        <input type="number" value={months} onChange={(e) => setMonths(Number(e.target.value))} />
        <button onClick={async () => {
          const resp = await simOverride({ capacity_request: { wafers_per_month: wpm, months } });
          alert(resp.capacity_summary);
        }}>Request</button>
      </div>
    </div>
  );
}

function AIPlan() {
  return (
    <div>
      <h2>AI Plan</h2>
      <button
        onClick={async () => {
          const p = await simPlanQuarter();
          alert(`${p.decisions.join(", ")} (score ${p.expected_score})`);
        }}
      >
        Fetch
      </button>
    </div>
  );
}

function Kpi({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ border: "1px solid #eee", borderRadius: 8, padding: 12 }}>
      <div style={{ fontSize: 12, color: "#666" }}>{label}</div>
      <div style={{ fontSize: 20 }}>{value}</div>
    </div>
  );
}
