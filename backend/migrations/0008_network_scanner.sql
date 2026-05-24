-- Network scanner: configuration, scan jobs, per-run results and current inventory.

INSERT INTO settings (key, value) VALUES
    ('network_scanner', '{
        "enabled": true,
        "ranges": ["10.10.0.0/23"],
        "exclude": [],
        "discovery": {
            "method": "auto",
            "dnsResolution": false,
            "maxRetries": 1,
            "hostTimeoutMs": 2500,
            "overallTimeoutSec": 120,
            "timingTemplate": 4,
            "minRate": 5000
        },
        "portScan": {
            "enabled": false,
            "profile": "fast",
            "ports": "22,53,80,443,445,3389,8006,8080,8443,9200",
            "serviceDetection": false,
            "osDetection": false,
            "scanTechnique": "syn",
            "udpScan": false,
            "onlyScanDiscovered": true,
            "skipHostDiscovery": true
        },
        "schedule": {
            "enabled": false,
            "intervalMinutes": 60,
            "runAtStart": false
        },
        "retentionDays": 90
    }'::jsonb)
ON CONFLICT (key) DO NOTHING;

CREATE TABLE network_scan_jobs (
    id          BIGSERIAL PRIMARY KEY,
    status      TEXT NOT NULL DEFAULT 'queued',
    trigger     TEXT NOT NULL DEFAULT 'manual',
    settings    JSONB NOT NULL,
    summary     JSONB,
    error       TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at  TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX network_scan_jobs_status_created_idx ON network_scan_jobs (status, created_at);
CREATE INDEX network_scan_jobs_created_idx ON network_scan_jobs (created_at DESC);

CREATE TABLE network_scan_devices (
    id               BIGSERIAL PRIMARY KEY,
    job_id           BIGINT NOT NULL REFERENCES network_scan_jobs(id) ON DELETE CASCADE,
    ip               TEXT NOT NULL,
    hostname         TEXT,
    mac              TEXT,
    vendor           TEXT,
    status           TEXT NOT NULL DEFAULT 'up',
    discovery_method TEXT NOT NULL DEFAULT 'unknown',
    latency_ms       DOUBLE PRECISION,
    ports            JSONB NOT NULL DEFAULT '[]'::jsonb,
    os_guess         TEXT,
    raw              JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_seen        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX network_scan_devices_job_ip_uq ON network_scan_devices (job_id, ip);
CREATE INDEX network_scan_devices_job_idx ON network_scan_devices (job_id);

CREATE TABLE network_scan_inventory (
    ip               TEXT PRIMARY KEY,
    hostname         TEXT,
    mac              TEXT,
    vendor           TEXT,
    status           TEXT NOT NULL DEFAULT 'up',
    discovery_method TEXT NOT NULL DEFAULT 'unknown',
    latency_ms       DOUBLE PRECISION,
    ports            JSONB NOT NULL DEFAULT '[]'::jsonb,
    os_guess         TEXT,
    first_seen       TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen        TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_job_id      BIGINT REFERENCES network_scan_jobs(id) ON DELETE SET NULL,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX network_scan_inventory_last_seen_idx ON network_scan_inventory (last_seen DESC);
