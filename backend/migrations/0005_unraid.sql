-- Cybex Sentinel — Unraid source support.
--
-- Adds Unraid GraphQL API sources, extends alert thresholds and stores a few
-- Unraid-specific rollup metrics for sparklines.

CREATE TABLE unraid_sources (
    id         BIGSERIAL PRIMARY KEY,
    name       TEXT NOT NULL,
    host       TEXT NOT NULL,
    api_key    TEXT NOT NULL,
    enabled    BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX unraid_sources_name_uq ON unraid_sources (name);

ALTER TABLE metric_samples
    ADD COLUMN unraid_servers_online DOUBLE PRECISION NOT NULL DEFAULT 0,
    ADD COLUMN unraid_array_used_pct DOUBLE PRECISION NOT NULL DEFAULT 0,
    ADD COLUMN unraid_array_used_tb DOUBLE PRECISION NOT NULL DEFAULT 0,
    ADD COLUMN unraid_containers_running DOUBLE PRECISION NOT NULL DEFAULT 0,
    ADD COLUMN unraid_vms_running DOUBLE PRECISION NOT NULL DEFAULT 0;

UPDATE settings
SET value = value || '{
    "unraid_cpu_warn": 90,
    "unraid_mem_warn": 90,
    "unraid_array_warn": 85,
    "unraid_disk_warn": 90,
    "unraid_temp_warn": 55,
    "unraid_temp_crit": 65
}'::jsonb,
updated_at = now()
WHERE key = 'alert_thresholds';
