import React, { useEffect, useState } from "react";
import { useAppStore } from "./store";
import { simTick, simPlanQuarter, simOverride, getSimLists, getSimState } from "./api";

export function App() {
  const { snapshot, setSnapshot, loading, setLoading, stateDto, setStateDto, lists, setLists, isBusy, setBusy, setError } = useAppStore();
  useEffect(() => {
    // initial load lists/state
    (async () => {
      try {
        setLists(await getSimLists());
        setStateDto(await getSimState());
      } catch (e) {}
    })();
  }, [setLists, setStateDto]);
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
            disabled={loading || isBusy}
            onClick={async () => {
              setLoading(true);
              setBusy(true);
              try {
                const snap = await simTick(1);
                setSnapshot(snap);
                setStateDto(await getSimState());
              } catch (e: any) {
                setError(e?.toString?.() ?? String(e));
              } finally {
                setLoading(false);
                setBusy(false);
              }
            }}
          >
            Tick Month
          </button>
        </div>
      </div>
      <div style={{ flex: 1, padding: 16 }}>
        <div style={{ marginBottom: 8, color: "#666" }}>
          {stateDto ? `Date ${stateDto.date} · Month #${stateDto.month_index}` : `Month #${snapshot?.months_run ?? 0}`}
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
  const { snapshot, stateDto, history } = useAppStore();
  if (!stateDto) return <div>No data yet.</div>;
  const kpi = stateDto.kpi;
  const data = history.map((h) => ({ m: h.months_run, revenue: h.revenue_cents / 100, profit: h.profit_cents / 100 }));
  return (
    <div>
      <h2>Dashboard</h2>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 12 }}>
        <Kpi label="Cash" value={cents(kpi.cash_cents)} />
        <Kpi label="Revenue" value={cents(kpi.revenue_cents)} />
        <Kpi label="COGS" value={cents(kpi.cogs_cents)} />
        <Kpi label="Contract Costs" value={cents(kpi.contract_costs_cents)} />
        <Kpi label="Profit" value={cents(kpi.profit_cents)} />
        <Kpi label="ASP" value={cents(stateDto.pricing.asp_cents)} />
        <Kpi label="Unit Cost" value={cents(stateDto.pricing.unit_cost_cents)} />
        <Kpi label="Share" value={`${(kpi.share * 100).toFixed(1)}%`} />
        <Kpi label="R&D" value={`${(kpi.rd_pct * 100).toFixed(1)}%`} />
        <Kpi label="Output" value={`${kpi.output_units}`} />
        <Kpi label="Inventory" value={`${kpi.inventory_units}`} />
      </div>
      <div style={{ marginTop: 16 }}>
        <h3>Revenue vs Profit</h3>
        <LineChart data={data} />
      </div>
    </div>
  );
}

function Markets() {
  const [delta, setDelta] = useState(0);
  return (
    <div>
      <h2>Markets</h2>
      <SegmentsTable />
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
      <QueueTable />
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
      <ContractsTable />
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

function SegmentsTable() {
  const { stateDto } = useAppStore();
  if (!stateDto) return null;
  return (
    <table style={{ width: "100%", margin: "8px 0" }}>
      <thead>
        <tr><th align="left">Segment</th><th>Base Demand</th><th>Elasticity</th></tr>
      </thead>
      <tbody>
        {stateDto.segments.map((s) => (
          <tr key={s.name}><td>{s.name}</td><td align="right">{s.base_demand_units}</td><td align="right">{s.price_elasticity}</td></tr>
        ))}
      </tbody>
    </table>
  );
}

function ContractsTable() {
  const { stateDto } = useAppStore();
  if (!stateDto) return null;
  return (
    <table style={{ width: "100%", margin: "8px 0" }}>
      <thead>
        <tr><th align="left">Foundry</th><th>Wafers/mo</th><th>Billing</th><th>ToP</th><th>Start</th><th>End</th></tr>
      </thead>
      <tbody>
        {stateDto.contracts.map((c, i) => (
          <tr key={i}><td>{c.foundry_id}</td><td align="right">{c.wafers_per_month}</td><td align="right">{c.billing_cents_per_wafer}c</td><td align="right">{Math.round(c.take_or_pay_frac * 100)}%</td><td>{c.start}</td><td>{c.end}</td></tr>
        ))}
      </tbody>
    </table>
  );
}

function QueueTable() {
  const { stateDto } = useAppStore();
  if (!stateDto) return null;
  return (
    <table style={{ width: "100%", margin: "8px 0" }}>
      <thead>
        <tr><th align="left">Tech</th><th>Start</th><th>Ready</th><th>Expedite</th><th>Cost</th><th>Perf</th></tr>
      </thead>
      <tbody>
        {stateDto.pipeline.queue.map((q, i) => (
          <tr key={i}><td>{q.tech_node}</td><td>{q.start}</td><td>{q.ready}</td><td align="center">{q.expedite ? "Yes" : "No"}</td><td align="right">{cents(q.expedite_cost_cents)}</td><td align="right">{q.perf_index}</td></tr>
        ))}
      </tbody>
    </table>
  );
}

function LineChart({ data }: { data: { m: number; revenue: number; profit: number }[] }) {
  // Minimal inline chart: print as CSV rows if recharts not available at runtime
  return (
    <pre style={{ background: "#fafafa", padding: 8 }}>
      {data.map((d) => `m${d.m}: R=${d.revenue.toFixed(0)} P=${d.profit.toFixed(0)}`).join("\n")}
    </pre>
  );
}
