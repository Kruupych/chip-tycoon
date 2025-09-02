-- Core tables for Chip Tycoon persistence (Phase 4)

CREATE TABLE IF NOT EXISTS saves (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  description TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS snapshots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  save_id INTEGER NOT NULL REFERENCES saves(id) ON DELETE CASCADE,
  month_index INTEGER NOT NULL,
  format TEXT NOT NULL CHECK (format IN ('bincode','json')),
  data BLOB NOT NULL,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS companies (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS tech_nodes (
  id TEXT PRIMARY KEY,
  year_available INTEGER NOT NULL,
  wafer_cost_usd REAL,
  mask_set_cost_usd REAL
);

CREATE TABLE IF NOT EXISTS markets (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE,
  base_demand_units INTEGER NOT NULL,
  price_elasticity REAL NOT NULL
);
