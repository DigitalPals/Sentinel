-- Cybex Sentinel — initial schema.
--
-- Everything that used to live in config.toml and in-memory state now lives
-- here: application settings, monitored sources (with their API credentials)
-- and the alert workflow state that previously did not survive a restart.

CREATE EXTENSION IF NOT EXISTS timescaledb;

-- ── Application settings ────────────────────────────────────────────────────
-- Key/value store. JSONB lets one table hold scalars, booleans and the small
-- grouped objects (alert thresholds, UI preferences). The backend reads the
-- whole table into a typed RuntimeConfig on every poll.
CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── Monitored sources ───────────────────────────────────────────────────────
-- Secrets are stored as plaintext columns (same exposure as the old
-- config.toml); the database file should be protected accordingly.
CREATE TABLE unifi_sources (
    id         BIGSERIAL PRIMARY KEY,
    name       TEXT NOT NULL DEFAULT 'UniFi',
    host       TEXT NOT NULL,
    api_key    TEXT NOT NULL,
    enabled    BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE proxmox_sources (
    id           BIGSERIAL PRIMARY KEY,
    name         TEXT NOT NULL,
    host         TEXT NOT NULL,
    token_id     TEXT NOT NULL,
    token_secret TEXT NOT NULL,
    enabled      BOOLEAN NOT NULL DEFAULT true,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX proxmox_sources_name_uq ON proxmox_sources (name);

-- ── Alert workflow state ────────────────────────────────────────────────────
-- Persists the per-alert bookkeeping (first-seen, re-fire count, ack/resolve
-- status) so acknowledgements survive a backend restart.
CREATE TABLE alert_state (
    alert_key   TEXT PRIMARY KEY,
    first_seen  BIGINT NOT NULL,
    last_seen   BIGINT NOT NULL,
    occurrences INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL DEFAULT 'open',
    assignee    TEXT,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── Seed defaults ───────────────────────────────────────────────────────────
-- Values mirror the constants that were previously hardcoded, so behaviour is
-- unchanged on a fresh database.
INSERT INTO settings (key, value) VALUES
    ('poll_interval_sec',      '15'::jsonb),
    ('bind',                   '"0.0.0.0:8787"'::jsonb),
    ('history_max_samples',    '6000'::jsonb),
    ('history_retention_days', '30'::jsonb),
    ('http_timeout_sec',       '12'::jsonb),
    ('frontend_poll_ms',       '5000'::jsonb),
    ('alert_thresholds', '{
        "guest_mem_crit": 92,
        "guest_mem_warn": 85,
        "guest_cpu_warn": 88,
        "guest_disk_warn": 90,
        "node_mem_crit": 90,
        "node_mem_warn": 80,
        "node_cpu_crit": 92,
        "node_cpu_warn": 85,
        "node_disk_warn": 90,
        "unifi_cpu_warn": 90,
        "unifi_mem_warn": 92
    }'::jsonb),
    ('ui_prefs', '{"accent": "cyan", "density": "regular", "showSpark": true}'::jsonb);
