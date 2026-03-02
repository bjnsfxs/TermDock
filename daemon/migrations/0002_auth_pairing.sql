-- 0002_auth_pairing.sql

CREATE TABLE IF NOT EXISTS auth_devices (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  platform      TEXT NULL,
  token_hash    TEXT NOT NULL UNIQUE,
  created_at    TEXT NOT NULL,
  last_seen_at  TEXT NOT NULL,
  revoked_at    TEXT NULL
);

CREATE INDEX IF NOT EXISTS idx_auth_devices_active
  ON auth_devices(revoked_at, created_at DESC);

CREATE TABLE IF NOT EXISTS pair_sessions (
  id                 TEXT PRIMARY KEY,
  pair_secret_hash   TEXT NOT NULL,
  status             TEXT NOT NULL,
  requested_name     TEXT NULL,
  platform           TEXT NULL,
  expires_at_epoch   INTEGER NOT NULL,
  issued_device_id   TEXT NULL,
  issued_token       TEXT NULL,
  approved_at        TEXT NULL,
  rejected_at        TEXT NULL,
  delivered_at       TEXT NULL,
  created_at         TEXT NOT NULL,
  updated_at         TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pair_sessions_status_expiry
  ON pair_sessions(status, expires_at_epoch);
