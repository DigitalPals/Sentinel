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
mod proxmox;
mod routes;
mod unifi;
mod unraid;

use std::sync::{Arc, RwLock};

use config::RuntimeConfig;
use engine::{build_clients, AlertStore, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cybex_sentinel=info,tower_http=warn".into()),
        )
        .init();

    // Connect to the database and bring the schema up to date.
    let pool = db::connect().await?;
    db::migrate(&pool).await?;
    db::setup_timescale(&pool).await;

    // One-time migration helper: `cybex-sentinel import-config` imports the
    // legacy config.toml + data/history.json into the database, then exits.
    if std::env::args().nth(1).as_deref() == Some("import-config") {
        return importer::run(&pool).await;
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

    let bind = config.bind.clone();
    let state = Arc::new(AppState {
        pool,
        config: RwLock::new(Arc::new(config)),
        clients: RwLock::new(clients),
        snapshot: RwLock::new(Arc::new(model::Snapshot::default())),
        history: RwLock::new(history),
        alerts: RwLock::new(alerts),
    });

    // Background poller: refreshes the snapshot on the configured interval.
    tokio::spawn(engine::run_poller(state.clone()));

    let app = routes::router(state);
    let addr: std::net::SocketAddr = bind
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid bind address '{bind}': {e}"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Cybex Sentinel listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
