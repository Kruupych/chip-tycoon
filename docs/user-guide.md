# Chip Tycoon — 10‑Minute Tutorial

- Start the 1990s campaign (Campaign → Restart 1990s).
- Dashboard shows KPIs; Mission HUD lists goals.
- Markets: adjust ASP ±5% to react to demand; keep margin ≥5%.
- Capacity: request foundry capacity (e.g., 1000 wafers/mo for 12 months).
- R&D / Tapeout: queue a tapeout; expedite to shorten by 3 months at cost.
- Tick by month/quarter/year and watch revenue/profit trends.

Difficulty: easy/normal/hard

- Affects AI min margin, price epsilon, player cash, market growth, and event severity.
- Set via Campaign → Difficulty. Presets load from `assets/scenarios/difficulty.yaml`.

Export & Autosaves

- Export: Campaign → Export Report (JSON/Parquet). Uses dry‑run; world state is not mutated.
- Autosaves: created once per quarter when enabled. Transactional status: `in_progress` → `done`.
- Rotation keeps the last 6 autosaves (oldest `auto-*` entries are deleted).

Hotkeys / Quick Actions

- Tick Month: run one month.
- Simulate Quarter: run three months and autosave (if enabled).
- Save/Load: open modal to manage saves (autosaves are labeled and show status).

