-- Placeholder migration; real schema will be added in Phase 4
CREATE TABLE IF NOT EXISTS schema_version (
  id INTEGER PRIMARY KEY,
  version INTEGER NOT NULL
);
