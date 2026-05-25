-- Cybex Sentinel — persisted event log stream.
--
-- Events can be noisy because many are observed on every poll. Store them as
-- coalesced 5-minute records keyed by source/target/message (or an explicit
-- event key from the collector), then let TimescaleDB handle chunking,
-- compression and retention.

CREATE TABLE event_logs (
    bucket      TIMESTAMPTZ NOT NULL,
    first_seen  TIMESTAMPTZ NOT NULL,
    last_seen   TIMESTAMPTZ NOT NULL,
    event_key   TEXT NOT NULL,
    level       TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source      TEXT NOT NULL,
    target      TEXT NOT NULL,
    msg         TEXT NOT NULL,
    seen_count  BIGINT NOT NULL DEFAULT 1,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Time-partition into 1-day chunks. `bucket` is the 5-minute coalescing bucket
-- for the event, so unique keys can be enforced per chunk without storing the
-- same poll-observed state every few seconds.
SELECT create_hypertable(
    'event_logs', 'bucket',
    chunk_time_interval => INTERVAL '1 day',
    if_not_exists => TRUE
);

CREATE UNIQUE INDEX event_logs_bucket_key_uq ON event_logs (bucket, event_key);
CREATE INDEX event_logs_last_seen_idx ON event_logs (last_seen DESC);
CREATE INDEX event_logs_source_level_idx ON event_logs (source_kind, level, last_seen DESC);

-- Keep raw event logs bounded. The dashboard is operational, not an audit-log
-- archive; alert workflow state is stored separately in `alert_state`.
SELECT add_retention_policy('event_logs', INTERVAL '30 days', if_not_exists => TRUE);

-- Compress older chunks. Segmenting by source/level keeps filtered log scans
-- efficient while shrinking repeated source and message data.
ALTER TABLE event_logs SET (
    timescaledb.compress,
    timescaledb.compress_orderby = 'bucket DESC, last_seen DESC',
    timescaledb.compress_segmentby = 'source_kind, level'
);
SELECT add_compression_policy('event_logs', INTERVAL '7 days', if_not_exists => TRUE);
