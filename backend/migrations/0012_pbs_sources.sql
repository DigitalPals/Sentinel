-- Add Proxmox Backup Server sources.

CREATE TABLE IF NOT EXISTS pbs_sources (
    id           BIGSERIAL PRIMARY KEY,
    name         TEXT NOT NULL,
    host         TEXT NOT NULL,
    token_id     TEXT NOT NULL,
    token_secret TEXT NOT NULL,
    enabled      BOOLEAN NOT NULL DEFAULT true,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS pbs_sources_name_uq ON pbs_sources (name);
