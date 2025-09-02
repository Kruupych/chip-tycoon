-- Capacity and tapeout structures

CREATE TABLE IF NOT EXISTS foundry_contracts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  save_id INTEGER NOT NULL REFERENCES saves(id) ON DELETE CASCADE,
  foundry_id TEXT NOT NULL,
  wafers_per_month INTEGER NOT NULL,
  price_per_wafer_cents INTEGER NOT NULL,
  take_or_pay_frac REAL NOT NULL,
  billing_cents_per_wafer INTEGER NOT NULL,
  billing_model TEXT NOT NULL,
  lead_time_months INTEGER NOT NULL,
  start TEXT NOT NULL,
  end TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_foundry_contracts_save ON foundry_contracts(save_id);

CREATE TABLE IF NOT EXISTS tapeout_queue (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  save_id INTEGER NOT NULL REFERENCES saves(id) ON DELETE CASCADE,
  product_json TEXT NOT NULL,
  tech_node TEXT NOT NULL,
  start TEXT NOT NULL,
  ready TEXT NOT NULL,
  expedite INTEGER NOT NULL,
  expedite_cost_cents INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS released_products (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  save_id INTEGER NOT NULL REFERENCES saves(id) ON DELETE CASCADE,
  product_json TEXT NOT NULL,
  released_at TEXT NOT NULL
);

