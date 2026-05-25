//! Cybex Sentinel — UniFi & Proxmox monitoring backend.
//!
//! Polls every configured source on a fixed interval, aggregates the result
//! into a single snapshot and serves it (plus the bundled frontend) over HTTP.
//! All configuration, API credentials and metric history live in
//! PostgreSQL/TimescaleDB — the only external input is the `DATABASE_URL`
//! connection string (which has a sensible default).

mod auth;
mod config;
mod db;
mod engine;
mod history;
mod importer;
mod model;
mod network_scanner;
mod notify;
mod proxmox;
mod routes;
mod secret;
mod unifi;
mod unraid;

use std::sync::{Arc, RwLock};

use chrono::Local;
use config::RuntimeConfig;
use engine::{build_clients, AlertStore, AppState, SourceRuntime};
use tracing_subscriber::fmt::{format::Writer, time::FormatTime};

struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", Local::now().format("%Y-%m-%dT%H:%M:%S%.6f%:z"))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_timer(LocalTimer)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cybex_sentinel=info,tower_http=warn".into()),
        )
        .init();

    // Connect to the database and bring the schema up to date.
    let pool = db::connect().await?;
    db::migrate(&pool).await?;
    db::setup_timescale(&pool).await;
    if secret::uses_fallback_key() {
        tracing::warn!(
            "SENTINEL_SECRET_KEY is not set; database credentials are encrypted with a local fallback key"
        );
    }
    db::protect_existing_secrets(&pool).await?;

    // One-time helpers / sidecar modes.
    match std::env::args().nth(1).as_deref() {
        Some("import-config") => {
            return importer::run(&pool).await;
        }
        Some("network-scanner-worker") => {
            return network_scanner::run_worker(pool).await;
        }
        _ => {}
    }

    // Best-effort housekeeping: drop login sessions that have already expired.
    match db::delete_expired_sessions(&pool).await {
        Ok(n) if n > 0 => tracing::info!("pruned {n} expired session(s)"),
        Ok(_) => {}
        Err(e) => tracing::warn!("could not prune expired sessions: {e:#}"),
    }

    let config = RuntimeConfig::load(&pool).await?;
    tracing::info!(
        "config loaded — {} Proxmox source(s), {} Unraid source(s), UniFi {}",
        config.proxmox.len(),
        config.unraid.len(),
        if config.unifi.is_some() {
            "configured"
        } else {
            "not configured"
        },
    );
    if config.unifi.is_none() && config.proxmox.is_empty() && config.unraid.is_empty() {
        tracing::warn!("no sources configured yet — add them on the Settings page");
    }
    db::configure_metric_retention(&pool, config.history_retention_days).await?;

    let clients = build_clients(&config);
    let history = history::History::new(
        db::recent_samples(&pool, config.history_max_samples).await?,
        config.history_max_samples,
    );
    let alerts = AlertStore::from_rows(db::load_alert_state(&pool).await?);
    let (snapshot_tx, _) = tokio::sync::broadcast::channel(32);

    let bind = config.bind.clone();
    let state = Arc::new(AppState {
        pool,
        config: RwLock::new(Arc::new(config)),
        clients: RwLock::new(clients),
        source_runtime: RwLock::new(SourceRuntime::default()),
        snapshot: RwLock::new(Arc::new(model::Snapshot::default())),
        snapshot_tx,
        history: RwLock::new(history),
        alerts: RwLock::new(alerts),
    });

    // Background poller: refreshes the snapshot on the configured interval.
    tokio::spawn(engine::run_poller(state.clone()));
    tokio::spawn(network_scanner::run_scheduler(state.clone()));

    let app = routes::router(state);
    let addr: std::net::SocketAddr = bind
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid bind address '{bind}': {e}"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Cybex Sentinel listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
