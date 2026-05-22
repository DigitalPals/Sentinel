//! PostgreSQL + TimescaleDB data access layer.
//!
//! All SQL lives here. The rest of the backend works with the typed structs
//! below and never touches `sqlx` directly. The database is the single source
//! of truth for settings, API credentials, the metric history and the alert
//! workflow state.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::{FromRow, PgPool, Row};

use crate::history::Sample;

/// Connection string used when `DATABASE_URL` is not set — matches the bundled
/// `docker-compose.yml`, so the common case needs no environment variables.
const DEFAULT_DATABASE_URL: &str = "postgres://sentinel:sentinel@localhost:5432/sentinel";

/// Connect to PostgreSQL, reading `DATABASE_URL` (or falling back to the
/// docker-compose default). This is the one piece of configuration that cannot
/// live in the database itself.
pub async fn connect() -> anyhow::Result<PgPool> {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string());
    tracing::info!("connecting to PostgreSQL at {}", redact(&url));
    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&url)
        .await
        .with_context(|| format!("connecting to PostgreSQL at {}", redact(&url)))
}

/// Apply pending schema migrations from `backend/migrations/`.
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("running database migrations")?;
    Ok(())
}

/// Create the hourly continuous aggregate over `metric_samples`.
///
/// Best-effort: continuous aggregates cannot be created inside the transaction
/// that wraps a migration, so this runs as standalone auto-committed statements
/// after `migrate`. A failure here is logged but never blocks startup.
pub async fn setup_timescale(pool: &PgPool) {
    const CAGG: &str = "CREATE MATERIALIZED VIEW IF NOT EXISTS metric_samples_hourly \
         WITH (timescaledb.continuous) AS \
         SELECT time_bucket(INTERVAL '1 hour', ts) AS bucket, \
                avg(wan_down_mbps)    AS wan_down_mbps, \
                avg(wan_up_mbps)      AS wan_up_mbps, \
                avg(availability)     AS availability, \
                avg(devices_online)   AS devices_online, \
                avg(active_alerts)    AS active_alerts, \
                avg(storage_tb)       AS storage_tb, \
                avg(wireless_clients) AS wireless_clients, \
                max(events_total)     AS events_total, \
                max(error_events)     AS error_events \
         FROM metric_samples GROUP BY bucket WITH NO DATA";
    if let Err(e) = sqlx::query(CAGG).execute(pool).await {
        tracing::warn!("could not create metric_samples_hourly continuous aggregate: {e}");
        return;
    }
    const POLICY: &str = "SELECT add_continuous_aggregate_policy('metric_samples_hourly', \
         start_offset => INTERVAL '1 month', \
         end_offset => INTERVAL '1 hour', \
         schedule_interval => INTERVAL '1 hour', \
         if_not_exists => TRUE)";
    if let Err(e) = sqlx::query(POLICY).execute(pool).await {
        tracing::warn!("could not add continuous-aggregate refresh policy: {e}");
    }
}

// ── Settings ────────────────────────────────────────────────────────────────

/// All rows of the `settings` table as a key → JSON map.
pub async fn get_settings_map(pool: &PgPool) -> anyhow::Result<HashMap<String, serde_json::Value>> {
    let rows = sqlx::query("SELECT key, value FROM settings")
        .fetch_all(pool)
        .await
        .context("loading settings")?;
    let mut map = HashMap::with_capacity(rows.len());
    for row in rows {
        map.insert(row.get::<String, _>("key"), row.get::<serde_json::Value, _>("value"));
    }
    Ok(map)
}

/// Insert or update one setting.
pub async fn set_setting(pool: &PgPool, key: &str, value: &serde_json::Value) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ($1, $2::jsonb, now()) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .with_context(|| format!("saving setting '{key}'"))?;
    Ok(())
}

// ── Sources ─────────────────────────────────────────────────────────────────

/// A configured UniFi source as stored in the database.
#[derive(Debug, Clone, FromRow)]
pub struct UnifiSourceRow {
    pub id: i64,
    pub name: String,
    pub host: String,
    pub api_key: String,
    pub enabled: bool,
}

/// A configured Proxmox source as stored in the database.
#[derive(Debug, Clone, FromRow)]
pub struct ProxmoxSourceRow {
    pub id: i64,
    pub name: String,
    pub host: String,
    pub token_id: String,
    pub token_secret: String,
    pub enabled: bool,
}

pub async fn get_unifi_sources(pool: &PgPool) -> anyhow::Result<Vec<UnifiSourceRow>> {
    sqlx::query_as::<_, UnifiSourceRow>(
        "SELECT id, name, host, api_key, enabled FROM unifi_sources ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .context("loading UniFi sources")
}

pub async fn get_unifi_source(pool: &PgPool, id: i64) -> anyhow::Result<Option<UnifiSourceRow>> {
    sqlx::query_as::<_, UnifiSourceRow>(
        "SELECT id, name, host, api_key, enabled FROM unifi_sources WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("loading UniFi source")
}

pub async fn insert_unifi_source(
    pool: &PgPool,
    name: &str,
    host: &str,
    api_key: &str,
    enabled: bool,
) -> anyhow::Result<i64> {
    let row = sqlx::query(
        "INSERT INTO unifi_sources (name, host, api_key, enabled) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(name)
    .bind(host)
    .bind(api_key)
    .bind(enabled)
    .fetch_one(pool)
    .await
    .context("inserting UniFi source")?;
    Ok(row.get("id"))
}

pub async fn update_unifi_source(
    pool: &PgPool,
    id: i64,
    name: &str,
    host: &str,
    api_key: &str,
    enabled: bool,
) -> anyhow::Result<bool> {
    let res = sqlx::query(
        "UPDATE unifi_sources SET name = $2, host = $3, api_key = $4, enabled = $5, \
         updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(name)
    .bind(host)
    .bind(api_key)
    .bind(enabled)
    .execute(pool)
    .await
    .context("updating UniFi source")?;
    Ok(res.rows_affected() > 0)
}

pub async fn delete_unifi_source(pool: &PgPool, id: i64) -> anyhow::Result<bool> {
    let res = sqlx::query("DELETE FROM unifi_sources WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("deleting UniFi source")?;
    Ok(res.rows_affected() > 0)
}

pub async fn get_proxmox_sources(pool: &PgPool) -> anyhow::Result<Vec<ProxmoxSourceRow>> {
    sqlx::query_as::<_, ProxmoxSourceRow>(
        "SELECT id, name, host, token_id, token_secret, enabled FROM proxmox_sources ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .context("loading Proxmox sources")
}

pub async fn get_proxmox_source(pool: &PgPool, id: i64) -> anyhow::Result<Option<ProxmoxSourceRow>> {
    sqlx::query_as::<_, ProxmoxSourceRow>(
        "SELECT id, name, host, token_id, token_secret, enabled FROM proxmox_sources WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("loading Proxmox source")
}

pub async fn insert_proxmox_source(
    pool: &PgPool,
    name: &str,
    host: &str,
    token_id: &str,
    token_secret: &str,
    enabled: bool,
) -> anyhow::Result<i64> {
    let row = sqlx::query(
        "INSERT INTO proxmox_sources (name, host, token_id, token_secret, enabled) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(name)
    .bind(host)
    .bind(token_id)
    .bind(token_secret)
    .bind(enabled)
    .fetch_one(pool)
    .await
    .context("inserting Proxmox source")?;
    Ok(row.get("id"))
}

pub async fn update_proxmox_source(
    pool: &PgPool,
    id: i64,
    name: &str,
    host: &str,
    token_id: &str,
    token_secret: &str,
    enabled: bool,
) -> anyhow::Result<bool> {
    let res = sqlx::query(
        "UPDATE proxmox_sources SET name = $2, host = $3, token_id = $4, token_secret = $5, \
         enabled = $6, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(name)
    .bind(host)
    .bind(token_id)
    .bind(token_secret)
    .bind(enabled)
    .execute(pool)
    .await
    .context("updating Proxmox source")?;
    Ok(res.rows_affected() > 0)
}

pub async fn delete_proxmox_source(pool: &PgPool, id: i64) -> anyhow::Result<bool> {
    let res = sqlx::query("DELETE FROM proxmox_sources WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("deleting Proxmox source")?;
    Ok(res.rows_affected() > 0)
}

// ── Users & sessions ────────────────────────────────────────────────────────

/// A user account as stored in the database.
#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
}

/// A live login session, joined with its owning user.
#[derive(Debug, Clone, FromRow)]
pub struct SessionRow {
    pub user_id: i64,
    pub username: String,
    pub expires_at: DateTime<Utc>,
}

/// How many user accounts exist. Zero means the app still needs first-run setup.
pub async fn count_users(pool: &PgPool) -> anyhow::Result<i64> {
    let row = sqlx::query("SELECT count(*) AS n FROM users")
        .fetch_one(pool)
        .await
        .context("counting users")?;
    Ok(row.get::<i64, _>("n"))
}

pub async fn insert_user(pool: &PgPool, username: &str, password_hash: &str) -> anyhow::Result<i64> {
    let row = sqlx::query("INSERT INTO users (username, password_hash) VALUES ($1, $2) RETURNING id")
        .bind(username)
        .bind(password_hash)
        .fetch_one(pool)
        .await
        .context("inserting user")?;
    Ok(row.get("id"))
}

/// Look up a user by name, case-insensitively.
pub async fn get_user_by_username(pool: &PgPool, username: &str) -> anyhow::Result<Option<UserRow>> {
    sqlx::query_as::<_, UserRow>(
        "SELECT id, username, password_hash FROM users WHERE lower(username) = lower($1)",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .context("loading user")
}

pub async fn create_session(
    pool: &PgPool,
    token_hash: &str,
    user_id: i64,
    expires_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(token_hash)
        .bind(user_id)
        .bind(expires_at)
        .execute(pool)
        .await
        .context("creating session")?;
    Ok(())
}

pub async fn get_session(pool: &PgPool, token_hash: &str) -> anyhow::Result<Option<SessionRow>> {
    sqlx::query_as::<_, SessionRow>(
        "SELECT s.user_id, u.username, s.expires_at FROM sessions s \
         JOIN users u ON u.id = s.user_id WHERE s.token_hash = $1",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
    .context("loading session")
}

pub async fn delete_session(pool: &PgPool, token_hash: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(token_hash)
        .execute(pool)
        .await
        .context("deleting session")?;
    Ok(())
}

/// Prune sessions whose expiry has passed — best-effort housekeeping on startup.
pub async fn delete_expired_sessions(pool: &PgPool) -> anyhow::Result<u64> {
    let res = sqlx::query("DELETE FROM sessions WHERE expires_at < now()")
        .execute(pool)
        .await
        .context("pruning expired sessions")?;
    Ok(res.rows_affected())
}

// ── Metric history ──────────────────────────────────────────────────────────

/// One row of `metric_samples`. Maps onto [`Sample`] (the in-memory form keeps
/// a Unix-epoch `t` instead of a `TIMESTAMPTZ`).
#[derive(FromRow)]
struct SampleRow {
    ts: DateTime<Utc>,
    wan_down_mbps: f64,
    wan_up_mbps: f64,
    availability: f64,
    devices_online: f64,
    devices_total: f64,
    active_alerts: f64,
    alerts_crit: f64,
    alerts_warn: f64,
    vm_count: f64,
    lxc_count: f64,
    nodes_online: f64,
    storage_tb: f64,
    wireless_clients: f64,
    wired_clients: f64,
    poe_ports: f64,
    events_total: f64,
    error_events: f64,
}

impl From<SampleRow> for Sample {
    fn from(r: SampleRow) -> Self {
        Sample {
            t: r.ts.timestamp(),
            wan_down_mbps: r.wan_down_mbps,
            wan_up_mbps: r.wan_up_mbps,
            availability: r.availability,
            devices_online: r.devices_online,
            devices_total: r.devices_total,
            active_alerts: r.active_alerts,
            alerts_crit: r.alerts_crit,
            alerts_warn: r.alerts_warn,
            vm_count: r.vm_count,
            lxc_count: r.lxc_count,
            nodes_online: r.nodes_online,
            storage_tb: r.storage_tb,
            wireless_clients: r.wireless_clients,
            wired_clients: r.wired_clients,
            poe_ports: r.poe_ports,
            events_total: r.events_total,
            error_events: r.error_events,
        }
    }
}

/// Append one metric sample. A duplicate timestamp is ignored, which keeps both
/// the poller and the one-time history import idempotent.
pub async fn insert_sample(pool: &PgPool, s: &Sample) -> anyhow::Result<()> {
    let ts = DateTime::from_timestamp(s.t, 0).unwrap_or_else(Utc::now);
    sqlx::query(
        "INSERT INTO metric_samples (ts, wan_down_mbps, wan_up_mbps, availability, \
         devices_online, devices_total, active_alerts, alerts_crit, alerts_warn, vm_count, \
         lxc_count, nodes_online, storage_tb, wireless_clients, wired_clients, poe_ports, \
         events_total, error_events) VALUES \
         ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) \
         ON CONFLICT (ts) DO NOTHING",
    )
    .bind(ts)
    .bind(s.wan_down_mbps)
    .bind(s.wan_up_mbps)
    .bind(s.availability)
    .bind(s.devices_online)
    .bind(s.devices_total)
    .bind(s.active_alerts)
    .bind(s.alerts_crit)
    .bind(s.alerts_warn)
    .bind(s.vm_count)
    .bind(s.lxc_count)
    .bind(s.nodes_online)
    .bind(s.storage_tb)
    .bind(s.wireless_clients)
    .bind(s.wired_clients)
    .bind(s.poe_ports)
    .bind(s.events_total)
    .bind(s.error_events)
    .execute(pool)
    .await
    .context("inserting metric sample")?;
    Ok(())
}

/// The most recent `limit` samples, returned oldest-first — the working set the
/// in-memory [`crate::history::History`] is seeded with.
pub async fn recent_samples(pool: &PgPool, limit: usize) -> anyhow::Result<Vec<Sample>> {
    let rows = sqlx::query_as::<_, SampleRow>(
        "SELECT ts, wan_down_mbps, wan_up_mbps, availability, devices_online, devices_total, \
         active_alerts, alerts_crit, alerts_warn, vm_count, lxc_count, nodes_online, storage_tb, \
         wireless_clients, wired_clients, poe_ports, events_total, error_events \
         FROM metric_samples ORDER BY ts DESC LIMIT $1",
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .context("loading recent metric samples")?;
    Ok(rows.into_iter().rev().map(Sample::from).collect())
}

// ── Alert state ─────────────────────────────────────────────────────────────

/// One persisted alert-workflow record.
#[derive(Debug, Clone, FromRow)]
pub struct AlertStateRow {
    pub alert_key: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub occurrences: i32,
    pub status: String,
    pub assignee: Option<String>,
}

pub async fn load_alert_state(pool: &PgPool) -> anyhow::Result<Vec<AlertStateRow>> {
    sqlx::query_as::<_, AlertStateRow>(
        "SELECT alert_key, first_seen, last_seen, occurrences, status, assignee FROM alert_state",
    )
    .fetch_all(pool)
    .await
    .context("loading alert state")
}

/// Replace the persisted alert state with `rows` (the live tracked set). Done in
/// one transaction so a restart always sees a consistent picture.
pub async fn save_alert_state(pool: &PgPool, rows: &[AlertStateRow]) -> anyhow::Result<()> {
    let mut tx = pool.begin().await.context("begin alert-state transaction")?;
    sqlx::query("DELETE FROM alert_state")
        .execute(&mut *tx)
        .await
        .context("clearing alert state")?;
    for r in rows {
        sqlx::query(
            "INSERT INTO alert_state (alert_key, first_seen, last_seen, occurrences, status, \
             assignee, updated_at) VALUES ($1, $2, $3, $4, $5, $6, now())",
        )
        .bind(&r.alert_key)
        .bind(r.first_seen)
        .bind(r.last_seen)
        .bind(r.occurrences)
        .bind(&r.status)
        .bind(&r.assignee)
        .execute(&mut *tx)
        .await
        .context("writing alert state")?;
    }
    tx.commit().await.context("commit alert-state transaction")?;
    Ok(())
}

/// Hide the password component of a connection string for logging.
fn redact(url: &str) -> String {
    match (url.find("://"), url.find('@')) {
        (Some(s), Some(at)) if at > s => {
            let creds = &url[s + 3..at];
            if let Some(colon) = creds.find(':') {
                format!("{}{}:****{}", &url[..s + 3], &creds[..colon], &url[at..])
            } else {
                url.to_string()
            }
        }
        _ => url.to_string(),
    }
}
