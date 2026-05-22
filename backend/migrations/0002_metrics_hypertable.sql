-- Cybex Sentinel — metric history time-series.
--
-- Replaces the rolling data/history.json file. A wide table is used because a
-- Sample is a fixed set of cluster-wide scalars that is always written and read
-- as a whole; one row per poll. The table is a TimescaleDB hypertable so the
-- series gets time-based chunking, retention and compression for free.
--
-- The hourly continuous aggregate is created at runtime by db::setup_timescale
-- instead of here, because continuous aggregates cannot be created inside the
-- transaction that wraps each migration.

CREATE TABLE metric_samples (
    ts               TIMESTAMPTZ NOT NULL,
    wan_down_mbps    DOUBLE PRECISION NOT NULL DEFAULT 0,
    wan_up_mbps      DOUBLE PRECISION NOT NULL DEFAULT 0,
    availability     DOUBLE PRECISION NOT NULL DEFAULT 0,
    devices_online   DOUBLE PRECISION NOT NULL DEFAULT 0,
    devices_total    DOUBLE PRECISION NOT NULL DEFAULT 0,
    active_alerts    DOUBLE PRECISION NOT NULL DEFAULT 0,
    alerts_crit      DOUBLE PRECISION NOT NULL DEFAULT 0,
    alerts_warn      DOUBLE PRECISION NOT NULL DEFAULT 0,
    vm_count         DOUBLE PRECISION NOT NULL DEFAULT 0,
    lxc_count        DOUBLE PRECISION NOT NULL DEFAULT 0,
    nodes_online     DOUBLE PRECISION NOT NULL DEFAULT 0,
    storage_tb       DOUBLE PRECISION NOT NULL DEFAULT 0,
    wireless_clients DOUBLE PRECISION NOT NULL DEFAULT 0,
    wired_clients    DOUBLE PRECISION NOT NULL DEFAULT 0,
    poe_ports        DOUBLE PRECISION NOT NULL DEFAULT 0,
    events_total     DOUBLE PRECISION NOT NULL DEFAULT 0,
    error_events     DOUBLE PRECISION NOT NULL DEFAULT 0
);

-- Time-partition into 1-day chunks (~5760 rows/chunk at a 15s poll).
SELECT create_hypertable(
    'metric_samples', 'ts',
    chunk_time_interval => INTERVAL '1 day',
    if_not_exists => TRUE
);

-- One sample per timestamp — makes the poller insert and the one-time
-- history.json import (ON CONFLICT DO NOTHING) idempotent, and serves the
-- "most recent N rows" query the working set is loaded with.
CREATE UNIQUE INDEX metric_samples_ts_uq ON metric_samples (ts DESC);

-- Drop raw samples older than 30 days.
SELECT add_retention_policy('metric_samples', INTERVAL '30 days', if_not_exists => TRUE);

-- Compress chunks older than 7 days to keep long-range storage compact.
ALTER TABLE metric_samples SET (
    timescaledb.compress,
    timescaledb.compress_orderby = 'ts DESC'
);
SELECT add_compression_policy('metric_samples', INTERVAL '7 days', if_not_exists => TRUE);
