-- Cybex Sentinel — event-log dedupe state.
--
-- `event_logs` is the bounded TimescaleDB history table. This companion table
-- keeps one current row per stable event key so poll-observed conditions (for
-- example an AP remaining offline) update their last-seen timestamp instead of
-- creating a repeated log row every coalescing bucket.

CREATE TABLE IF NOT EXISTS event_log_state (
    event_key   TEXT PRIMARY KEY,
    first_seen  TIMESTAMPTZ NOT NULL,
    last_seen   TIMESTAMPTZ NOT NULL,
    level       TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source      TEXT NOT NULL,
    target      TEXT NOT NULL,
    msg         TEXT NOT NULL,
    seen_count  BIGINT NOT NULL DEFAULT 1,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS event_log_state_last_seen_idx ON event_log_state (last_seen DESC);
CREATE INDEX IF NOT EXISTS event_log_state_source_level_idx ON event_log_state (source_kind, level, last_seen DESC);

-- Backfill state from any rows captured by the initial hypertable-only
-- implementation.
WITH agg AS (
    SELECT event_key, min(first_seen) AS first_seen, max(last_seen) AS last_seen, sum(seen_count) AS seen_count
    FROM event_logs
    GROUP BY event_key
),
latest AS (
    SELECT DISTINCT ON (event_key)
        event_key, level, source_kind, source, target, msg
    FROM event_logs
    ORDER BY event_key, last_seen DESC, bucket DESC
)
INSERT INTO event_log_state (
    event_key, first_seen, last_seen, level, source_kind, source, target, msg, seen_count
)
SELECT
    agg.event_key,
    agg.first_seen,
    agg.last_seen,
    latest.level,
    latest.source_kind,
    latest.source,
    latest.target,
    latest.msg,
    agg.seen_count
FROM agg
JOIN latest USING (event_key)
ON CONFLICT (event_key) DO NOTHING;
