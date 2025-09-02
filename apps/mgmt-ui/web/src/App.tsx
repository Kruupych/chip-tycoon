import React, { useEffect, useState } from "react";
import { useAppStore } from "./store";
import { simTick, simPlanQuarter, simOverride, getSimLists, getSimState, simCampaignReset, simBalanceInfo, simCampaignSetDifficulty, simTutorialState, TutorialDto } from "./api";
import { invoke } from "@tauri-apps/api/core";
import { QueryClient, QueryClientProvider, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { LineChart as RLineChart, Line, XAxis, YAxis, Tooltip, Legend, ResponsiveContainer } from "recharts";

export function App() {
  const { snapshot, setSnapshot, loading, setLoading, stateDto, setStateDto, lists, setLists, isBusy, setBusy, setError } = useAppStore();
  const [nav, setNav] = useState<"dashboard" | "tutorial" | "campaign" | "markets" | "rd" | "capacity" | "ai">(
    "dashboard"
  );
  const [qc] = useState(() => new QueryClient());
  return (
    <QueryClientProvider client={qc}>
      <InnerApp nav={nav} setNav={setNav} />
    </QueryClientProvider>
  );
}

function InnerApp({ nav, setNav }: { nav: any; setNav: (v: any) => void }) {
  const { snapshot, setSnapshot, loading, setLoading, stateDto, setStateDto, lists, setLists, isBusy, setBusy, setError } = useAppStore();
  const qc = useQueryClient();
  const [showSave, setShowSave] = useState(false);
  const [tut, setTut] = useState<TutorialDto | null>(null);
  // Lists and state queries
  useQuery({
    queryKey: ["sim_lists"],
    queryFn: getSimLists,
    onSuccess: (data) => setLists(data),
  });
  useQuery({
    queryKey: ["sim_state"],
    queryFn: getSimState,
    onSuccess: (data) => setStateDto(data),
  });
  useQuery({
    queryKey: ["sim_tutorial"],
    queryFn: simTutorialState,
    onSuccess: (data) => setTut(data),
  });
  const refetchState = async () => {
    await qc.invalidateQueries({ queryKey: ["sim_state"] });
    await qc.invalidateQueries({ queryKey: ["sim_tutorial"] });
  };
  // Mutations
  const tickMut = useMutation({
    mutationFn: async () => simTick(1),
    onMutate: () => setBusy(true),
    onSuccess: async (snap) => {
      setSnapshot(snap);
      await refetchState();
    },
    onError: (e: any) => setError(e?.toString?.() ?? String(e)),
    onSettled: () => setBusy(false),
  });
  const quarterMut = useMutation({
    mutationFn: async () => (window as any).__tauriInvoke?.("sim_tick_quarter") ?? (await import("@tauri-apps/api/core")).invoke("sim_tick_quarter"),
    onMutate: () => setBusy(true),
    onSuccess: async () => {
      await refetchState();
    },
    onError: (e: any) => setError(e?.toString?.() ?? String(e)),
    onSettled: () => setBusy(false),
  });
  const yearMut = useMutation({
    mutationFn: async () => simTick(12),
    onMutate: () => setBusy(true),
    onSuccess: async () => {
      await refetchState();
    },
    onError: (e: any) => setError(e?.toString?.() ?? String(e)),
    onSettled: () => setBusy(false),
  });
  const overrideMut = useMutation({
    mutationFn: simOverride,
    onMutate: () => setBusy(true),
    onSuccess: async () => {
      await refetchState();
    },
    onError: (e: any) => setError(e?.toString?.() ?? String(e)),
    onSettled: () => setBusy(false),
  });
  return (
    <div style={{ display: "flex", height: "100vh", fontFamily: "sans-serif" }}>
      <div style={{ width: 220, borderRight: "1px solid #ddd", padding: 12 }}>
        <h3>Mgmt</h3>
        <div>
          {[
            ["dashboard", "Dashboard"],
            ["tutorial", "Tutorial"],
            ["campaign", "Campaign"],
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
          <button disabled={loading || isBusy} onClick={() => tickMut.mutate()}>Tick Month</button>
          <button style={{ marginLeft: 8 }} disabled={loading || isBusy} onClick={() => quarterMut.mutate()}>Simulate Quarter</button>
          <button style={{ marginLeft: 8 }} disabled={loading || isBusy} onClick={() => yearMut.mutate()}>Simulate Year</button>
          <button style={{ marginLeft: 8 }} onClick={() => setShowSave(true)}>Save/Load…</button>
        </div>
      </div>
      <div style={{ flex: 1, padding: 16 }}>
        <div style={{ marginBottom: 8, color: "#666" }}>
          {stateDto ? `Date ${stateDto.date} · Month #${stateDto.month_index}` : `Month #${snapshot?.months_run ?? 0}`}
        </div>
        {nav === "dashboard" && <Dashboard tut={tut} onGoto={(p)=>setNav(p as any)} />}
        {nav === "tutorial" && <TutorialPage tut={tut} onGoto={(p)=>setNav(p as any)} />}
        {nav === "campaign" && <Campaign />}
        {nav === "markets" && <Markets onOverride={(p)=>overrideMut.mutate(p as any)} />}
        {nav === "rd" && <RD onOverride={(p)=>overrideMut.mutate(p as any)} />}
        {nav === "capacity" && <Capacity onOverride={(p)=>overrideMut.mutate(p as any)} />}
        {nav === "ai" && <AIPlan onQuarter={() => quarterMut.mutate()} onOverride={(p)=>overrideMut.mutate(p as any)} />}
      </div>
      {showSave && <SaveLoadModal onClose={()=>setShowSave(false)} />}
    </div>
  );
}

function cents(n?: number) {
  return `$${((n ?? 0) / 100).toFixed(2)}`;
}

function Dashboard({ tut, onGoto }: { tut: TutorialDto | null; onGoto: (p: string) => void }) {
  const { snapshot, stateDto, history } = useAppStore();
  if (!stateDto) return <div>No data yet.</div>;
  const kpi = stateDto.kpi;
  const data = history.map((h) => ({ m: h.months_run, revenue: h.revenue_cents / 100, profit: h.profit_cents / 100 }));
  return (
    <div>
      <h2>Dashboard</h2>
      <MissionHUD />
      <TutorialHUD tut={tut} onGoto={onGoto} />
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
          <h3>Revenue vs Profit (last 24 months)</h3>
          <RechartsLine data={data.slice(-24)} />
        </div>
    </div>
  );
}

function MissionHUD() {
  const { stateDto } = useAppStore();
  const goals = (stateDto as any)?.campaign?.goals ?? [];
  if (!goals.length) return null;
  return (
    <div style={{ padding: 8, border: "1px solid #eee", margin: "8px 0", borderRadius: 6 }}>
      <strong>Mission HUD:</strong>
      <ul style={{ margin: 0 }}>
        {goals.slice(0, 3).map((g: any, i: number) => (
          <li key={i}>{g.desc} — {(g.progress * 100).toFixed(0)}% (by {g.deadline})</li>
        ))}
      </ul>
    </div>
  );
}

function TutorialHUD({ tut, onGoto }: { tut: TutorialDto | null; onGoto: (p: string) => void }) {
  if (!tut?.active || !tut.steps?.length) return null;
  const idx = Math.min(tut.current_step, tut.steps.length);
  const next = tut.steps[idx] ?? null;
  if (!next) return null;
  return (
    <div style={{ padding: 8, border: "1px dashed #cbd5e1", margin: "8px 0", borderRadius: 6, background: "#f8fafc" }}>
      <strong>Next step:</strong> {next.desc}
      <button style={{ marginLeft: 8 }} onClick={() => onGoto(next.nav_page)}>Go to {next.nav_label}</button>
      <div style={{ fontSize: 12, color: "#64748b", marginTop: 4 }}>{next.hint}</div>
    </div>
  );
}

function TutorialPage({ tut, onGoto }: { tut: TutorialDto | null; onGoto: (p: string) => void }) {
  return (
    <div>
      <h2>Tutorial</h2>
      {!tut?.active ? (
        <div>
          <p>No tutorial loaded. You can load it via Campaign -> Restart with tutorial scenario.</p>
          <button onClick={() => simCampaignReset("assets/scenarios/tutorial_24m.yaml")}>Load Tutorial</button>
        </div>
      ) : (
        <div>
          <ol>
            {tut.steps.map((s, i) => (
              <li key={s.id} style={{ margin: "8px 0" }}>
                <span style={{ padding: "2px 6px", borderRadius: 4, background: s.done ? "#dcfce7" : "#fee2e2" }}>{s.done ? "Done" : "Pending"}</span>
                <span style={{ marginLeft: 8 }}>
                  {s.desc}
                </span>
                <div style={{ fontSize: 12, color: "#64748b" }}>{s.hint}</div>
                {!s.done && (
                  <button style={{ marginTop: 4 }} onClick={() => onGoto(s.nav_page)}>Go to {s.nav_label}</button>
                )}
              </li>
            ))}
          </ol>
        </div>
      )}
    </div>
  );
}

function Campaign() {
  const { stateDto } = useAppStore();
  const camp = (stateDto as any)?.campaign as any;
  const [fmt, setFmt] = useState<"json"|"parquet">("json");
  const [path, setPath] = useState("telemetry/campaign_export.json");
  return (
    <div>
      <h2>Campaign</h2>
      <div style={{ marginBottom: 8 }}>
        <button onClick={() => simCampaignReset("1990s")}>Restart 1990s Campaign</button>
        <span style={{ marginLeft: 12 }}>
          Difficulty:
          <select onChange={(e) => simCampaignSetDifficulty(e.target.value)} defaultValue={camp?.difficulty ?? "normal"} style={{ marginLeft: 6 }}>
            <option value="easy">Easy</option>
            <option value="normal">Normal</option>
            <option value="hard">Hard</option>
          </select>
        </span>
        <span style={{ marginLeft: 12 }}>
          <label>Export: </label>
          <select value={fmt} onChange={(e)=>{ const f = e.target.value as any; setFmt(f); setPath(f === "json" ? "telemetry/campaign_export.json" : "telemetry/campaign_export.parquet"); }}>
            <option value="json">JSON</option>
            <option value="parquet">Parquet</option>
          </select>
          <button style={{ marginLeft: 6 }} onClick={async ()=>{ try { await invoke("sim_export_campaign", { path, format: fmt }); alert("Exported to " + path); } catch(e) { alert("Export failed: "+e); } }}>Export Report</button>
        </span>
      </div>
      {camp ? (
        <div>
          <div>Status: <span style={{ padding: "2px 6px", borderRadius: 4, background: camp.status === "Success" ? "#d1fae5" : camp.status === "Failed" ? "#fee2e2" : "#e5e7eb" }}>{camp.status}</span></div>
          <div>Start: {camp.start} — End: {camp.end}</div>
          <h3>Goals</h3>
          <table style={{ width: "100%", margin: "8px 0" }}>
            <thead><tr><th align="left">Goal</th><th>Progress</th><th>Deadline</th><th>Status</th></tr></thead>
            <tbody>
              {camp.goals.map((g: any, i: number) => (
                <tr key={i}><td>{g.desc}</td><td align="right">{(g.progress * 100).toFixed(0)}%</td><td>{g.deadline}</td><td align="center">{g.done ? "Done" : "InProgress"}</td></tr>
              ))}
            </tbody>
          </table>
          <ActiveModsTable />
        </div>
      ) : (
        <div>No campaign loaded.</div>
      )}
    </div>
  );
}

function Markets({ onOverride }: { onOverride: (p: any) => void }) {
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
        <button onClick={async () => { onOverride({ price_delta_frac: delta / 100 }); }}>
          Apply
        </button>
      </div>
    </div>
  );
}

function RD({ onOverride }: { onOverride: (p: any) => void }) {
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
        <button onClick={() => onOverride({ rd_delta_cents: rd })}>Apply</button>
      </div>
      <div style={{ marginTop: 12 }}>
        <label>Tech node: </label>
        <input value={tech} onChange={(e) => setTech(e.target.value)} />
        <label> Expedite </label>
        <input type="checkbox" checked={expedite} onChange={(e) => setExpedite(e.target.checked)} />
        <button onClick={() => onOverride({ tapeout: { perf_index: 0.8, die_area_mm2: 100, tech_node: tech, expedite } })}>Queue Tapeout</button>
      </div>
    </div>
  );
}

function Capacity({ onOverride }: { onOverride: (p: any) => void }) {
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
        <button onClick={() => onOverride({ capacity_request: { wafers_per_month: wpm, months } })}>Request</button>
      </div>
    </div>
  );
}

function AIPlan({ onQuarter, onOverride }: { onQuarter: () => void; onOverride: (p: any) => void }) {
  const [plan, setPlan] = useState<{ decisions: string[]; expected_score: number }>();
  useEffect(() => { (async () => { setPlan(await simPlanQuarter()); })(); }, []);
  return (
    <div>
      <h2>AI Plan</h2>
      <div style={{ marginBottom: 8 }}>
        <button onClick={onQuarter}>Simulate Quarter</button>
      </div>
      {plan && (
        <div>
          <div>Score: {plan.expected_score.toFixed(3)}</div>
          <ul>
            {plan.decisions.map((d, i) => (
              <li key={i}>{d}</li>
            ))}
          </ul>
          <button onClick={() => {
            // heuristic mapping: if decision starts with ASP, apply price; if Capacity, add capacity; if Tapeout, schedule tapeout
            const top = plan.decisions[0] || "";
            if (top.startsWith("ASP")) {
              const sign = top.includes("-") ? -1 : top.includes("+") ? 1 : 0;
              onOverride({ price_delta_frac: 0.05 * sign });
            } else if (top.startsWith("Capacity+")) {
              onOverride({ capacity_request: { wafers_per_month: 1000, months: 12 } });
            } else if (top.startsWith("Tapeout")) {
              onOverride({ tapeout: { perf_index: 0.8, die_area_mm2: 100, tech_node: "N90", expedite: top.includes("expedite") } });
            }
          }}>Apply Top Decision</button>
        </div>
      )}
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
  const forecast = (s: any) => {
    const g = (s.trend_pct ?? 0) / 100;
    const pts: number[] = [];
    let base = s.base_demand_t ?? s.base_demand_units;
    for (let i = 0; i < 12; i++) {
      pts.push(Math.floor(base));
      base = base * (1 + g / 12);
    }
    return pts;
  };
  return (
    <table style={{ width: "100%", margin: "8px 0" }}>
      <thead>
        <tr>
          <th align="left">Segment</th>
          <th>Base Demand (1990)</th>
          <th>Base Demand (t)</th>
          <th>Ref Price (t)</th>
          <th>Elasticity</th>
          <th>Trend</th>
          <th>Sold (t)</th>
          <th>Forecast 12m</th>
        </tr>
      </thead>
      <tbody>
        {stateDto.segments.map((s) => {
          const pts = forecast(s);
          return (
            <tr key={s.name}>
              <td>{s.name}</td>
              <td align="right">{s.base_demand_units}</td>
              <td align="right">{s.base_demand_t}</td>
              <td align="right">{cents(s.ref_price_t_cents)}</td>
              <td align="right">{s.elasticity.toFixed(2)}</td>
              <td align="right">{(s.trend_pct ?? 0).toFixed(1)}%</td>
              <td align="right">{s.sold_units}</td>
              <td><small>{pts.join(", ")}</small></td>
            </tr>
          );
        })}
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

function RechartsLine({ data }: { data: { m: number; revenue: number; profit: number }[] }) {
  return (
    <div style={{ width: "100%", height: 240 }}>
      <ResponsiveContainer>
        <RLineChart data={data}>
          <XAxis dataKey="m" />
          <YAxis />
          <Tooltip />
          <Legend />
          <Line type="monotone" dataKey="revenue" stroke="#8884d8" dot={false} />
          <Line type="monotone" dataKey="profit" stroke="#82ca9d" dot={false} />
        </RLineChart>
      </ResponsiveContainer>
    </div>
  );
}

function ActiveModsTable() {
  const [mods, setMods] = React.useState<{ id: string; kind: string; target: string; start: string; end: string }[]>([]);
  useEffect(() => { (async () => { try { const info = await simBalanceInfo(); setMods(info.active_mods); } catch {} })(); }, []);
  if (!mods.length) return null;
  return (
    <div>
      <h3>Active Mods</h3>
      <table style={{ width: "100%", margin: "8px 0" }}>
        <thead><tr><th align="left">ID</th><th>Type</th><th>Target</th><th>Start</th><th>End</th></tr></thead>
        <tbody>
          {mods.map((m, i) => (
            <tr key={i}><td>{m.id}</td><td align="center">{m.kind}</td><td>{m.target}</td><td>{m.start}</td><td>{m.end}</td></tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function SaveLoadModal({ onClose }: { onClose: ()=>void }) {
  const [saves, setSaves] = useState<{ id: number; name: string; created_at: string; progress: number }[]>([]);
  const [name, setName] = useState("");
  const [autosave, setAutosave] = useState(true);
  useEffect(() => { (async () => { try { const list = await invoke<any[]>("sim_list_saves"); setSaves(list as any); } catch {} })(); }, []);
  useEffect(() => { (async () => { try { await invoke("sim_set_autosave", { on: autosave }); } catch {} })(); }, [autosave]);
  return (
    <div style={{ position: "fixed", inset: 0, background: "rgba(0,0,0,0.3)", display: "flex", alignItems: "center", justifyContent: "center" }}>
      <div style={{ background: "white", padding: 16, borderRadius: 8, width: 600 }}>
        <div style={{ display: "flex", justifyContent: "space-between" }}>
          <h3>Save / Load</h3>
          <button onClick={onClose}>×</button>
        </div>
        <div style={{ marginBottom: 8 }}>
          <label>Name: </label>
          <input value={name} onChange={(e)=>setName(e.target.value)} placeholder="manual-..." />
          <button style={{ marginLeft: 8 }} onClick={async ()=>{ try { await invoke("sim_save", { name }); const list = await invoke<any[]>("sim_list_saves"); setSaves(list as any); } catch(e) { console.error(e);} }}>Save</button>
          <label style={{ marginLeft: 16 }}>
            <input type="checkbox" checked={autosave} onChange={(e)=>setAutosave(e.target.checked)} /> Autosave per quarter
          </label>
        </div>
        <table style={{ width: "100%" }}>
          <thead><tr><th align="left">Name</th><th>Created</th><th>Progress</th><th></th></tr></thead>
          <tbody>
            {saves.map(s => (
              <tr key={s.id}>
                <td>{s.name}</td>
                <td>{s.created_at}</td>
                <td align="center">{s.progress}</td>
                <td align="right"><button onClick={async ()=>{ try { await invoke("sim_load", { saveId: s.id }); onClose(); } catch(e){ console.error(e); } }}>Load</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
