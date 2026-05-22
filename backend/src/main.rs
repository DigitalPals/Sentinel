//! Cybex Sentinel — UniFi & Proxmox monitoring backend.
//!
//! Polls every configured source on a fixed interval, aggregates the result
//! into a single snapshot and serves it (plus the bundled frontend) over HTTP.

mod config;
mod engine;
mod history;
mod model;
mod proxmox;
mod routes;
mod unifi;

use std::sync::{Arc, RwLock};

use engine::{AlertStore, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cybex_sentinel=info,tower_http=warn".into()),
        )
        .init();

    let config = config::Config::load()?;
    tracing::info!(
        "config loaded — {} Proxmox host(s), UniFi {}",
        config.proxmox.len(),
        if config.unifi.is_some() {
            "configured"
        } else {
            "not configured"
        },
    );

    let proxmox = config
        .proxmox
        .iter()
        .map(proxmox::ProxmoxClient::new)
        .collect::<anyhow::Result<Vec<_>>>()?;
    let unifi = match &config.unifi {
        Some(c) => Some(unifi::UnifiClient::new(c)?),
        None => None,
    };

    let bind = config.bind.clone();
    let state = Arc::new(AppState {
        config,
        proxmox,
        unifi,
        snapshot: RwLock::new(Arc::new(model::Snapshot::default())),
        history: RwLock::new(history::History::load()),
        alerts: RwLock::new(AlertStore::default()),
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
