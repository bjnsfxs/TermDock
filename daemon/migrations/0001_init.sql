-- 0001_init.sql
-- NOTE: keep this migration idempotent for easy local dev.

CREATE TABLE IF NOT EXISTS instances (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  enabled       INTEGER NOT NULL DEFAULT 1,

  command       TEXT NOT NULL,
  args_json     TEXT NOT NULL DEFAULT '[]',
  cwd           TEXT NULL,
  env_json      TEXT NOT NULL DEFAULT '{}',
  use_pty       INTEGER NOT NULL DEFAULT 1,

  config_mode     TEXT NOT NULL DEFAULT 'none',  -- none | path | inline
  config_path     TEXT NULL,
  config_filename TEXT NULL,
  config_content  TEXT NULL,

  restart_policy TEXT NOT NULL DEFAULT 'never',  -- never | on-failure | always
  auto_start     INTEGER NOT NULL DEFAULT 0,

  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_instances_name ON instances(name);
