//! Polling engine — turns raw Proxmox/UniFi data into the served [`Snapshot`].
//!
//! A background task polls every source on a fixed interval, builds the full
//! snapshot (KPIs, nodes, guests, devices, topology, alerts, events) and stores
//! it behind an `RwLock`. HTTP handlers only ever read the latest snapshot.

mod alerts;
mod bandwidth;
mod events;
mod format;
mod proxmox_view;
mod topology;
mod unifi_view;
mod unraid_view;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use chrono::{TimeZone, Utc};
use sqlx::PgPool;
use tokio::sync::broadcast;

pub use alerts::{patch_alerts, AlertStore};

use self::alerts::{build_alerts_view, build_dashboard_issues, sev_rank, Candidate};
use self::bandwidth::build_bandwidth;
use self::events::build_events_view;
use self::format::{fmt_ago, fmt_mbps, fmt_mem, kpi, pct};
use self::proxmox_view::process_proxmox;
use self::topology::count_topology;
use self::unifi_view::process_unifi;
use self::unraid_view::process_unraid;
use crate::bmc::{BmcClient, BmcData};
use crate::config::RuntimeConfig;
use crate::db;
use crate::history::{History, Sample};
use crate::model::*;
use crate::notify;
use crate::pbs::{PbsClient, PbsData};
use crate::proxmox::{ProxmoxClient, ProxmoxData};
use crate::unifi::{UnifiClient, UnifiData};
use crate::unraid::{UnraidClient, UnraidData};

/// The HTTP clients built from the current source set.
pub struct Clients {
    pub proxmox: Vec<ProxmoxClient>,
    pub pbs: Vec<PbsClient>,
    pub bmc: Vec<BmcClient>,
    pub unraid: Vec<UnraidClient>,
    pub unifi: Option<UnifiClient>,
    /// `RuntimeConfig::source_sig` of the config these were built from.
    pub source_sig: u64,
}

#[derive(Default)]
pub struct SourceRuntime {
    entries: HashMap<String, SourceRuntimeEntry>,
    proxmox_last: HashMap<String, ProxmoxData>,
    pbs_last: HashMap<String, PbsData>,
    bmc_last: HashMap<String, BmcData>,
    unraid_last: HashMap<String, UnraidData>,
    unifi_last: Option<UnifiData>,
}

#[derive(Default)]
struct SourceRuntimeEntry {
    failure_count: u32,
    last_ok: Option<i64>,
    next_retry: i64,
    last_error: Option<String>,
}

impl SourceRuntime {
    fn retry_in(&self, key: &str, now: i64) -> Option<u64> {
        let entry = self.entries.get(key)?;
        (entry.next_retry > now).then_some((entry.next_retry - now) as u64)
    }

    fn record_success(&mut self, key: &str, now: i64) {
        let entry = self.entries.entry(key.to_string()).or_default();
        entry.failure_count = 0;
        entry.last_ok = Some(now);
        entry.next_retry = 0;
        entry.last_error = None;
    }

    fn record_failure(&mut self, key: &str, error: String, now: i64) {
        let entry = self.entries.entry(key.to_string()).or_default();
        entry.failure_count = entry.failure_count.saturating_add(1);
        let exp = entry.failure_count.saturating_sub(1).min(6);
        let backoff = (5u64.saturating_mul(2u64.saturating_pow(exp))).min(300);
        entry.next_retry = now + backoff as i64;
        entry.last_error = Some(error);
    }

    fn annotate_sources(&self, sources: &mut [SourceHealth], now: i64) {
        for source in sources {
            let key = source_key(&source.kind, &source.name);
            let Some(entry) = self.entries.get(&key) else {
                continue;
            };
            source.failure_count = entry.failure_count;
            source.retry_in_sec =
                (entry.next_retry > now).then_some((entry.next_retry - now) as u64);
            source.last_ok_ago_sec = entry.last_ok.map(|ts| now.saturating_sub(ts) as u64);
            if entry.failure_count > 0 && entry.last_ok.is_some() {
                source.ok = false;
                source.stale = true;
                source.detail = "showing last known good data".to_string();
                source.error = entry.last_error.clone();
            }
        }
    }
}

fn source_key(kind: &str, name: &str) -> String {
    format!("{kind}:{name}")
}

/// Build the Proxmox/UniFi HTTP clients for `cfg`. A source that fails to build
/// is skipped (and logged) rather than aborting, so the rest keep working.
pub fn build_clients(cfg: &RuntimeConfig) -> Clients {
    let mut proxmox = Vec::new();
    for pc in &cfg.proxmox {
        match ProxmoxClient::new(pc, cfg.http_timeout_sec) {
            Ok(c) => proxmox.push(c),
            Err(e) => tracing::error!("skipping Proxmox source '{}': {e:#}", pc.name),
        }
    }
    let mut pbs = Vec::new();
    for pc in &cfg.pbs {
        match PbsClient::new(pc, cfg.http_timeout_sec) {
            Ok(c) => pbs.push(c),
            Err(e) => tracing::error!("skipping PBS source '{}': {e:#}", pc.name),
        }
    }
    let mut bmc = Vec::new();
    for bc in &cfg.bmc {
        match BmcClient::new(bc, cfg.http_timeout_sec) {
            Ok(c) => bmc.push(c),
            Err(e) => tracing::error!("skipping BMC source '{}': {e:#}", bc.name),
        }
    }
    let mut unraid = Vec::new();
    for uc in &cfg.unraid {
        match UnraidClient::new(uc, cfg.http_timeout_sec) {
            Ok(c) => unraid.push(c),
            Err(e) => tracing::error!("skipping Unraid source '{}': {e:#}", uc.name),
        }
    }
    let unifi = match &cfg.unifi {
        Some(uc) => match UnifiClient::new(uc, cfg.http_timeout_sec) {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::error!("skipping UniFi source: {e:#}");
                None
            }
        },
        None => None,
    };
    Clients {
        proxmox,
        pbs,
        bmc,
        unraid,
        unifi,
        source_sig: cfg.source_sig(),
    }
}

/// Shared application state.
pub struct AppState {
    pub pool: PgPool,
    /// Latest configuration, reloaded from the database on every poll.
    pub config: RwLock<Arc<RuntimeConfig>>,
    /// HTTP clients, rebuilt when the source set changes.
    pub clients: RwLock<Clients>,
    pub source_runtime: RwLock<SourceRuntime>,
    pub snapshot: RwLock<Arc<Snapshot>>,
    pub snapshot_tx: broadcast::Sender<Arc<Snapshot>>,
    pub history: RwLock<History>,
    pub alerts: RwLock<AlertStore>,
}

impl AppState {
    pub fn current(&self) -> Arc<Snapshot> {
        self.snapshot.read().unwrap().clone()
    }

    pub fn config(&self) -> Arc<RuntimeConfig> {
        self.config.read().unwrap().clone()
    }
}

pub async fn run_poller(state: Arc<AppState>) {
    loop {
        let started = Instant::now();

        // Reload configuration from the database; reuse the last good config
        // if the database is briefly unreachable.
        let cfg = match RuntimeConfig::load(&state.pool).await {
            Ok(c) => Arc::new(c),
            Err(e) => {
                tracing::warn!("could not reload configuration from database: {e:#}");
                state.config()
            }
        };

        // Rebuild HTTP clients when the source set (or HTTP timeout) changed,
        // so credential edits made in the Settings page apply within one poll.
        let want_sig = cfg.source_sig();
        let stale = state.clients.read().unwrap().source_sig != want_sig;
        if stale {
            let clients = build_clients(&cfg);
            tracing::info!(
                "sources changed — {} Proxmox client(s), {} PBS client(s), {} BMC client(s), {} Unraid client(s), UniFi {}",
                clients.proxmox.len(),
                clients.pbs.len(),
                clients.bmc.len(),
                clients.unraid.len(),
                if clients.unifi.is_some() {
                    "configured"
                } else {
                    "not configured"
                },
            );
            *state.clients.write().unwrap() = clients;
        }
        *state.config.write().unwrap() = cfg.clone();

        let interval = cfg.poll_interval_sec.max(5);
        poll_once(&state, &cfg).await;

        let wait = Duration::from_secs(interval).saturating_sub(started.elapsed());
        tokio::time::sleep(wait).await;
    }
}

async fn poll_once(state: &Arc<AppState>, cfg: &RuntimeConfig) {
    // Take cheap clones of the clients so no lock is held across an await.
    let (proxmox, pbs, bmc, unraid, unifi) = {
        let c = state.clients.read().unwrap();
        (
            c.proxmox.clone(),
            c.pbs.clone(),
            c.bmc.clone(),
            c.unraid.clone(),
            c.unifi.clone(),
        )
    };

    // Query Proxmox hosts and UniFi concurrently.
    let pmx_futs = proxmox.iter().map(|c| {
        let state = state.clone();
        async move { collect_proxmox(c, &state).await }
    });
    let pbs_futs = pbs.iter().map(|c| {
        let state = state.clone();
        async move { collect_pbs(c, &state).await }
    });
    let bmc_futs = bmc.iter().map(|c| {
        let state = state.clone();
        async move { collect_bmc(c, &state).await }
    });
    let unraid_futs = unraid.iter().map(|c| {
        let state = state.clone();
        async move { collect_unraid(c, &state).await }
    });
    let state_for_unifi = state.clone();
    let unifi_fut = async {
        match &unifi {
            Some(c) => Some(collect_unifi(c, &state_for_unifi).await),
            None => None,
        }
    };
    let (pmx, pbs_res, bmc_res, unraid_res, unifi_res) = futures::join!(
        futures::future::join_all(pmx_futs),
        futures::future::join_all(pbs_futs),
        futures::future::join_all(bmc_futs),
        futures::future::join_all(unraid_futs),
        unifi_fut
    );

    let (mut snapshot, sample, notifications) = {
        let mut history = state.history.write().unwrap();
        let mut store = state.alerts.write().unwrap();
        build(
            cfg,
            &pmx,
            &pbs_res,
            &bmc_res,
            &unraid_res,
            &unifi_res,
            &mut history,
            &mut store,
        )
    };
    state
        .source_runtime
        .read()
        .unwrap()
        .annotate_sources(&mut snapshot.sources, Utc::now().timestamp());

    // Persist the new sample to the time-series table and the reconciled alert
    // workflow state; failures are logged but never fatal.
    if let Err(e) = db::insert_sample(&state.pool, &sample).await {
        tracing::warn!("could not persist metric sample: {e:#}");
    }
    let alert_rows = state.alerts.read().unwrap().rows();
    if let Err(e) = db::save_alert_state(&state.pool, &alert_rows).await {
        tracing::warn!("could not persist alert state: {e:#}");
    }
    if let Err(e) = db::save_event_logs(&state.pool, &snapshot.events.events).await {
        tracing::warn!("could not persist event logs: {e:#}");
    } else {
        match db::recent_event_logs(&state.pool, 220).await {
            Ok(events) => {
                let history = state.history.read().unwrap();
                snapshot.events = build_events_view(events, &history);
            }
            Err(e) => tracing::warn!("could not load persisted event logs: {e:#}"),
        }
    }
    let ok_sources = snapshot.sources.iter().filter(|s| s.ok).count();
    tracing::info!(
        "poll complete — {ok_sources}/{} sources up, {} alerts, {} events",
        snapshot.sources.len(),
        snapshot.alerts.alerts.len(),
        snapshot.events.events.len(),
    );

    let snapshot = Arc::new(snapshot);
    *state.snapshot.write().unwrap() = snapshot.clone();
    let _ = state.snapshot_tx.send(snapshot);

    if !notifications.is_empty() {
        if let Err(e) =
            notify::send_alert_notifications(&state.pool, &cfg.notifications, &notifications).await
        {
            tracing::warn!("could not send alert notification(s): {e:#}");
        }
    }
}

async fn collect_proxmox(
    client: &ProxmoxClient,
    state: &Arc<AppState>,
) -> (String, Result<ProxmoxData, String>) {
    let name = client.name.clone();
    let key = source_key("proxmox", &name);
    let now = Utc::now().timestamp();
    if let Some(retry) = state.source_runtime.read().unwrap().retry_in(&key, now) {
        if let Some(data) = state
            .source_runtime
            .read()
            .unwrap()
            .proxmox_last
            .get(&key)
            .cloned()
        {
            return (name, Ok(data));
        }
        return (name, Err(format!("backing off; retry in {retry}s")));
    }
    match client.collect().await {
        Ok(data) => {
            let mut runtime = state.source_runtime.write().unwrap();
            runtime.record_success(&key, now);
            runtime.proxmox_last.insert(key, data.clone());
            (name, Ok(data))
        }
        Err(e) => {
            let error = format!("{e:#}");
            let mut runtime = state.source_runtime.write().unwrap();
            runtime.record_failure(&key, error.clone(), now);
            if let Some(data) = runtime.proxmox_last.get(&key).cloned() {
                (name, Ok(data))
            } else {
                (name, Err(error))
            }
        }
    }
}

async fn collect_pbs(
    client: &PbsClient,
    state: &Arc<AppState>,
) -> (String, Result<PbsData, String>) {
    let name = client.name.clone();
    let key = source_key("pbs", &name);
    let now = Utc::now().timestamp();
    if let Some(retry) = state.source_runtime.read().unwrap().retry_in(&key, now) {
        if let Some(data) = state
            .source_runtime
            .read()
            .unwrap()
            .pbs_last
            .get(&key)
            .cloned()
        {
            return (name, Ok(data));
        }
        return (name, Err(format!("backing off; retry in {retry}s")));
    }
    match client.collect().await {
        Ok(data) => {
            let mut runtime = state.source_runtime.write().unwrap();
            runtime.record_success(&key, now);
            runtime.pbs_last.insert(key, data.clone());
            (name, Ok(data))
        }
        Err(e) => {
            let error = format!("{e:#}");
            let mut runtime = state.source_runtime.write().unwrap();
            runtime.record_failure(&key, error.clone(), now);
            if let Some(data) = runtime.pbs_last.get(&key).cloned() {
                (name, Ok(data))
            } else {
                (name, Err(error))
            }
        }
    }
}

async fn collect_bmc(
    client: &BmcClient,
    state: &Arc<AppState>,
) -> (String, Result<BmcData, String>) {
    let name = client.name.clone();
    let key = source_key("bmc", &name);
    let now = Utc::now().timestamp();
    if let Some(retry) = state.source_runtime.read().unwrap().retry_in(&key, now) {
        if let Some(data) = state
            .source_runtime
            .read()
            .unwrap()
            .bmc_last
            .get(&key)
            .cloned()
        {
            return (name, Ok(data));
        }
        return (name, Err(format!("backing off; retry in {retry}s")));
    }
    match client.collect().await {
        Ok(data) => {
            let mut runtime = state.source_runtime.write().unwrap();
            runtime.record_success(&key, now);
            runtime.bmc_last.insert(key, data.clone());
            (name, Ok(data))
        }
        Err(e) => {
            let error = format!("{e:#}");
            let mut runtime = state.source_runtime.write().unwrap();
            runtime.record_failure(&key, error.clone(), now);
            if let Some(data) = runtime.bmc_last.get(&key).cloned() {
                (name, Ok(data))
            } else {
                (name, Err(error))
            }
        }
    }
}

async fn collect_unraid(
    client: &UnraidClient,
    state: &Arc<AppState>,
) -> (String, Result<UnraidData, String>) {
    let name = client.name.clone();
    let key = source_key("unraid", &name);
    let now = Utc::now().timestamp();
    if let Some(retry) = state.source_runtime.read().unwrap().retry_in(&key, now) {
        if let Some(data) = state
            .source_runtime
            .read()
            .unwrap()
            .unraid_last
            .get(&key)
            .cloned()
        {
            return (name, Ok(data));
        }
        return (name, Err(format!("backing off; retry in {retry}s")));
    }
    match client.collect().await {
        Ok(data) => {
            let mut runtime = state.source_runtime.write().unwrap();
            runtime.record_success(&key, now);
            runtime.unraid_last.insert(key, data.clone());
            (name, Ok(data))
        }
        Err(e) => {
            let error = format!("{e:#}");
            let mut runtime = state.source_runtime.write().unwrap();
            runtime.record_failure(&key, error.clone(), now);
            if let Some(data) = runtime.unraid_last.get(&key).cloned() {
                (name, Ok(data))
            } else {
                (name, Err(error))
            }
        }
    }
}

async fn collect_unifi(client: &UnifiClient, state: &Arc<AppState>) -> Result<UnifiData, String> {
    let key = source_key("unifi", "UniFi");
    let now = Utc::now().timestamp();
    if let Some(retry) = state.source_runtime.read().unwrap().retry_in(&key, now) {
        if let Some(data) = state.source_runtime.read().unwrap().unifi_last.clone() {
            return Ok(data);
        }
        return Err(format!("backing off; retry in {retry}s"));
    }
    match client.collect().await {
        Ok(data) => {
            let mut runtime = state.source_runtime.write().unwrap();
            runtime.record_success(&key, now);
            runtime.unifi_last = Some(data.clone());
            Ok(data)
        }
        Err(e) => {
            let error = format!("{e:#}");
            let mut runtime = state.source_runtime.write().unwrap();
            runtime.record_failure(&key, error.clone(), now);
            if let Some(data) = runtime.unifi_last.clone() {
                Ok(data)
            } else {
                Err(error)
            }
        }
    }
}

fn build(
    config: &RuntimeConfig,
    pmx: &[(String, Result<ProxmoxData, String>)],
    pbs: &[(String, Result<PbsData, String>)],
    bmc: &[(String, Result<BmcData, String>)],
    unraid: &[(String, Result<UnraidData, String>)],
    unifi: &Option<Result<UnifiData, String>>,
    history: &mut History,
    store: &mut AlertStore,
) -> (Snapshot, Sample, Vec<Alert>) {
    let now = Utc::now().timestamp();
    let mut sources: Vec<SourceHealth> = Vec::new();

    // ── Proxmox ──────────────────────────────────────────────────────────
    let mut nodes: Vec<NodeTile> = Vec::new();
    let mut node_guests: Vec<NodeGuests> = Vec::new();
    let mut all_guests: Vec<Guest> = Vec::new();
    let mut events: Vec<Event> = Vec::new();
    let mut cands: Vec<Candidate> = Vec::new();
    let mut storage_used: u64 = 0;
    let mut storage_total: u64 = 0;
    let mut quorum_labels: Vec<String> = Vec::new();

    for (name, result) in pmx {
        match result {
            Err(e) => sources.push(SourceHealth {
                name: name.clone(),
                kind: "proxmox".to_string(),
                ok: false,
                stale: false,
                failure_count: 1,
                retry_in_sec: None,
                last_ok_ago_sec: None,
                detail: "unreachable".to_string(),
                error: Some(e.clone()),
            }),
            Ok(data) => {
                let processed = process_proxmox(data, now, &config.thresholds);
                let node_count = processed.nodes.len();
                nodes.extend(processed.nodes);
                for ng in processed.guest_groups {
                    all_guests.extend(ng.guests.iter().cloned());
                    node_guests.push(ng);
                }
                events.extend(processed.events);
                cands.extend(processed.candidates);
                storage_used += processed.storage_used;
                storage_total += processed.storage_total;
                if let Some(label) = proxmox_quorum_label(data) {
                    if !quorum_labels.contains(&label) {
                        quorum_labels.push(label);
                    }
                }
                sources.push(SourceHealth {
                    name: name.clone(),
                    kind: "proxmox".to_string(),
                    ok: true,
                    stale: false,
                    failure_count: 0,
                    retry_in_sec: None,
                    last_ok_ago_sec: Some(0),
                    detail: format!("PVE {} · {} node(s)", data.release, node_count),
                    error: None,
                });
            }
        }
    }

    // ── Proxmox Backup Server ───────────────────────────────────────────
    for (name, result) in pbs {
        match result {
            Err(e) => sources.push(SourceHealth {
                name: name.clone(),
                kind: "pbs".to_string(),
                ok: false,
                stale: false,
                failure_count: 1,
                retry_in_sec: None,
                last_ok_ago_sec: None,
                detail: "unreachable".to_string(),
                error: Some(e.clone()),
            }),
            Ok(data) => {
                let max_used = data.datastores.iter().filter_map(|d| d.used_pct()).max();
                let usage = max_used
                    .map(|pct| format!(" · max datastore {pct}% used"))
                    .unwrap_or_default();
                sources.push(SourceHealth {
                    name: name.clone(),
                    kind: "pbs".to_string(),
                    ok: true,
                    stale: false,
                    failure_count: 0,
                    retry_in_sec: None,
                    last_ok_ago_sec: Some(0),
                    detail: format!(
                        "PBS {} · {} datastore(s){usage}",
                        data.version,
                        data.datastores.len()
                    ),
                    error: None,
                });
                for store in &data.datastores {
                    if store.used_pct().is_some_and(|pct| pct >= 85) {
                        cands.push(Candidate {
                            key: format!("pbs:{}:datastore:{}:usage", data.server, store.store),
                            sev: "warn".to_string(),
                            source: "pbs".to_string(),
                            host: data.server.clone(),
                            target: store.store.clone(),
                            title: format!("PBS datastore {} is filling up", store.store),
                            desc: format!(
                                "Datastore {} on {} is {}% used.",
                                store.store,
                                data.server,
                                store.used_pct().unwrap_or(0)
                            ),
                            rule: "pbs.datastore.used_pct >= 85".to_string(),
                        });
                    }
                }
            }
        }
    }

    // ── BMC / IPMI / Redfish ─────────────────────────────────────────────
    let mut bmc_controllers: Vec<BmcControllerOut> = Vec::new();
    let mut bmc_sensors: Vec<BmcSensorOut> = Vec::new();
    let mut bmc_drives: Vec<BmcDriveOut> = Vec::new();
    for (name, result) in bmc {
        match result {
            Err(e) => sources.push(SourceHealth {
                name: name.clone(),
                kind: "bmc".to_string(),
                ok: false,
                stale: false,
                failure_count: 1,
                retry_in_sec: None,
                last_ok_ago_sec: None,
                detail: "unreachable".to_string(),
                error: Some(e.clone()),
            }),
            Ok(data) => {
                let ok = data.health.eq_ignore_ascii_case("ok") || data.health == "Unknown";
                sources.push(SourceHealth {
                    name: data.source_name.clone(),
                    kind: "bmc".to_string(),
                    ok,
                    stale: false,
                    failure_count: 0,
                    retry_in_sec: None,
                    last_ok_ago_sec: Some(0),
                    detail: format!(
                        "{} BMC {} · {} · {} sensor(s)",
                        data.manufacturer, data.manager_firmware, data.power_state, data.sensors.len()
                    ),
                    error: None,
                });
                if !ok {
                    cands.push(Candidate {
                        key: format!("bmc:{}:health", data.source_name),
                        sev: "crit".to_string(),
                        source: "bmc".to_string(),
                        host: data.host.clone(),
                        target: data.source_name.clone(),
                        title: format!("{} BMC health is {}", data.source_name, data.health),
                        desc: "Redfish reports a non-OK system health state.".to_string(),
                        rule: "bmc.health != OK".to_string(),
                    });
                }
                for sensor in &data.sensors {
                    let tone = if sensor.status == "ok" { "ok" } else { "warn" };
                    if sensor.status != "ok" {
                        cands.push(Candidate {
                            key: format!("bmc:{}:sensor:{}", data.source_name, sensor.name),
                            sev: "warn".to_string(),
                            source: "bmc".to_string(),
                            host: data.host.clone(),
                            target: sensor.name.clone(),
                            title: format!("BMC sensor {} is {}", sensor.name, sensor.status),
                            desc: format!("{} reports {}.", sensor.name, sensor.raw),
                            rule: "bmc.sensor.status != ok".to_string(),
                        });
                    }
                    bmc_sensors.push(BmcSensorOut {
                        source: data.source_name.clone(),
                        name: sensor.name.clone(),
                        kind: sensor.kind.clone(),
                        status: sensor.status.clone(),
                        reading: sensor.reading,
                        unit: sensor.unit.clone(),
                        raw: sensor.raw.clone(),
                        tone: tone.to_string(),
                    });
                }
                for drive in &data.drives {
                    let tone = if drive.health.eq_ignore_ascii_case("ok") { "ok" } else { "warn" };
                    bmc_drives.push(BmcDriveOut {
                        source: data.source_name.clone(),
                        name: drive.name.clone(),
                        manufacturer: drive.manufacturer.clone(),
                        model: drive.model.clone(),
                        serial: drive.serial.clone(),
                        capacity: fmt_mem(drive.capacity_bytes),
                        health: drive.health.clone(),
                        state: drive.state.clone(),
                        tone: tone.to_string(),
                    });
                }
                bmc_controllers.push(BmcControllerOut {
                    name: data.source_name.clone(),
                    host: data.host.clone(),
                    manufacturer: data.manufacturer.clone(),
                    model: data.model.clone(),
                    serial: data.serial.clone(),
                    bios_version: data.bios_version.clone(),
                    power_state: data.power_state.clone(),
                    health: data.health.clone(),
                    processor_count: data.processor_count,
                    memory_gib: data.memory_gib,
                    manager_model: data.manager_model.clone(),
                    manager_firmware: data.manager_firmware.clone(),
                    manager_uuid: data.manager_uuid.clone(),
                    ipmi_available: data.ipmi_available,
                    ipmi_power: data.ipmi_power.clone().unwrap_or_default(),
                    ipmi_manufacturer: data.ipmi_device.as_ref().map(|i| i.manufacturer.clone()).unwrap_or_default(),
                    ipmi_product: data.ipmi_device.as_ref().map(|i| i.product.clone()).unwrap_or_default(),
                    ipmi_firmware: data.ipmi_device.as_ref().map(|i| i.firmware.clone()).unwrap_or_default(),
                    ipmi_version: data.ipmi_device.as_ref().map(|i| i.ipmi_version.clone()).unwrap_or_default(),
                });
            }
        }
    }

    // ── UniFi ────────────────────────────────────────────────────────────
    let mut unifi_view = UnifiView::default();
    let mut topology = TopoNode::default();
    let mut wan_down = 0.0;
    let mut wan_up = 0.0;
    match unifi {
        None => {}
        Some(Err(e)) => sources.push(SourceHealth {
            name: "UniFi".to_string(),
            kind: "unifi".to_string(),
            ok: false,
            stale: false,
            failure_count: 1,
            retry_in_sec: None,
            last_ok_ago_sec: None,
            detail: "unreachable".to_string(),
            error: Some(e.clone()),
        }),
        Some(Ok(data)) => {
            let processed = process_unifi(data, now, &config.thresholds);
            let device_count = processed.view.devices.len();
            wan_down = processed.wan_down_mbps;
            wan_up = processed.wan_up_mbps;
            events.extend(processed.events);
            cands.extend(processed.candidates);
            sources.push(SourceHealth {
                name: "UniFi".to_string(),
                kind: "unifi".to_string(),
                ok: true,
                stale: false,
                failure_count: 0,
                retry_in_sec: None,
                last_ok_ago_sec: Some(0),
                detail: format!("Network {} · {} devices", data.app_version, device_count),
                error: None,
            });
            unifi_view = processed.view;
            topology = processed.topology;
        }
    }

    // ── Unraid ──────────────────────────────────────────────────────────
    let mut unraid_servers: Vec<UnraidServerOut> = Vec::new();
    let mut unraid_containers_running = 0u32;
    let mut unraid_containers_total = 0u32;
    let mut unraid_vms_running = 0u32;
    let mut unraid_vms_total = 0u32;
    let mut unraid_storage_used_pct_sum = 0.0;
    let mut unraid_storage_used_tb = 0.0;
    let mut unraid_storage_warn = 0u32;
    let mut unraid_software_update_count = 0u32;
    for (name, result) in unraid {
        match result {
            Err(e) => sources.push(SourceHealth {
                name: name.clone(),
                kind: "unraid".to_string(),
                ok: false,
                stale: false,
                failure_count: 1,
                retry_in_sec: None,
                last_ok_ago_sec: None,
                detail: "unreachable".to_string(),
                error: Some(e.clone()),
            }),
            Ok(data) => {
                let processed = process_unraid(data, now, &config.thresholds);
                unraid_containers_running += processed.containers_running;
                unraid_containers_total += processed.containers_total;
                unraid_vms_running += processed.vms_running;
                unraid_vms_total += processed.vms_total;
                unraid_software_update_count += processed.software_update_count;
                unraid_storage_used_pct_sum += processed.storage_used_pct as f64;
                unraid_storage_used_tb += processed.storage_used_tb;
                if processed.storage_used_pct >= config.thresholds.unraid_array_warn {
                    unraid_storage_warn += 1;
                }
                events.extend(processed.events);
                cands.extend(processed.candidates);
                sources.push(SourceHealth {
                    name: data.source_name.clone(),
                    kind: "unraid".to_string(),
                    ok: true,
                    stale: false,
                    failure_count: 0,
                    retry_in_sec: None,
                    last_ok_ago_sec: Some(0),
                    detail: format!(
                        "Unraid {} · array {} · {} container(s)",
                        data.version,
                        data.array.state,
                        data.containers.len()
                    ),
                    error: None,
                });
                unraid_servers.push(processed.server);
            }
        }
    }
    let unraid_servers_online = unraid_servers.iter().filter(|s| s.status == "ok").count() as u32;
    let unraid_servers_total = unraid_servers.len() as u32;
    let unraid_storage_used_pct = if unraid_servers_total == 0 {
        0.0
    } else {
        unraid_storage_used_pct_sum / unraid_servers_total as f64
    };

    // ── PBS backup assurance view ─────────────────────────────────────────
    let mut pbs_datastores: Vec<PbsDatastoreOut> = Vec::new();
    let mut pbs_backups: Vec<PbsBackupOut> = Vec::new();
    let mut pbs_tasks: Vec<PbsTaskOut> = Vec::new();
    let mut latest_by_guest: HashMap<(String, String), crate::pbs::PbsSnapshot> = HashMap::new();
    let mut backup_keys: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut pbs_has_data = false;
    let mut failed_tasks_24h = 0u32;
    for (_name, result) in pbs {
        let Ok(data) = result else { continue };
        pbs_has_data = true;
        let mut groups_by_store: HashMap<&str, u32> = HashMap::new();
        let mut snapshots_by_store: HashMap<&str, u32> = HashMap::new();
        for group in &data.groups {
            *groups_by_store.entry(group.datastore.as_str()).or_default() += 1;
        }
        for snap in &data.snapshots {
            *snapshots_by_store
                .entry(snap.datastore.as_str())
                .or_default() += 1;
        }
        for store in &data.datastores {
            let used_pct = store.used_pct().unwrap_or(0);
            pbs_datastores.push(PbsDatastoreOut {
                source: data.server.clone(),
                store: store.store.clone(),
                used: fmt_mem(store.used.unwrap_or(0)),
                total: fmt_mem(store.total.unwrap_or(0)),
                avail: fmt_mem(store.avail.unwrap_or(0)),
                used_pct,
                groups: groups_by_store
                    .get(store.store.as_str())
                    .copied()
                    .unwrap_or(0),
                snapshots: snapshots_by_store
                    .get(store.store.as_str())
                    .copied()
                    .unwrap_or(0),
                status: if used_pct >= 85 {
                    "warn".to_string()
                } else {
                    "ok".to_string()
                },
                gc_status: store.gc_status.clone().unwrap_or_else(|| "—".to_string()),
                maintenance_mode: store
                    .maintenance_mode
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
            });
        }
        for snap in &data.snapshots {
            let age_min = ((now - snap.backup_time).max(0)) / 60;
            let verification = snap
                .verification
                .as_ref()
                .and_then(|v| v.state.clone())
                .unwrap_or_else(|| "unverified".to_string());
            let guest_kind = if snap.backup_type == "ct" {
                "lxc"
            } else {
                "vm"
            };
            let guest_id = snap.backup_id.parse::<u64>().unwrap_or(0);
            let guest = all_guests
                .iter()
                .find(|g| g.kind == guest_kind && g.id == guest_id);
            let key = (snap.backup_type.clone(), snap.backup_id.clone());
            backup_keys.insert(key.clone());
            latest_by_guest
                .entry(key)
                .and_modify(|existing| {
                    if snap.backup_time > existing.backup_time {
                        *existing = snap.clone();
                    }
                })
                .or_insert_with(|| snap.clone());
            pbs_backups.push(PbsBackupOut {
                id: format!(
                    "{}:{}:{}:{}/{}:{}",
                    data.server,
                    snap.datastore,
                    snap.namespace,
                    snap.backup_type,
                    snap.backup_id,
                    snap.backup_time
                ),
                source: data.server.clone(),
                namespace: if snap.namespace.is_empty() {
                    "root".to_string()
                } else {
                    snap.namespace.clone()
                },
                backup_type: snap.backup_type.clone(),
                backup_id: snap.backup_id.clone(),
                guest_name: guest
                    .map(|g| g.name.clone())
                    .or_else(|| snap.comment.clone())
                    .unwrap_or_default(),
                guest_node: guest.map(|g| g.node.clone()).unwrap_or_default(),
                guest_server: guest.map(|g| g.server.clone()).unwrap_or_default(),
                backup_time: Utc
                    .timestamp_opt(snap.backup_time, 0)
                    .single()
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_default(),
                age_min,
                size: fmt_mem(snap.size.unwrap_or(0)),
                size_bytes: snap.size.unwrap_or(0),
                verification,
                protected: snap.protected,
                owner: snap.owner.clone().unwrap_or_default(),
            });
        }
        for task in data.tasks.iter().take(50) {
            let age_min = ((now - task.starttime).max(0)) / 60;
            let status = task.status.clone().unwrap_or_else(|| "running".to_string());
            let ok = status == "OK" || status == "running";
            if !ok && now.saturating_sub(task.starttime) <= 86400 {
                failed_tasks_24h += 1;
            }
            pbs_tasks.push(PbsTaskOut {
                id: task.upid.clone(),
                source: data.server.clone(),
                worker_type: task.worker_type.clone(),
                worker_id: task.worker_id.clone().unwrap_or_default(),
                status,
                start_time: Utc
                    .timestamp_opt(task.starttime, 0)
                    .single()
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_default(),
                age_min,
                duration_sec: task.endtime.map(|end| end.saturating_sub(task.starttime)),
                user: task.user.clone().unwrap_or_default(),
                ok,
            });
        }
    }
    pbs_backups.sort_by(|a, b| a.age_min.cmp(&b.age_min));
    pbs_backups.truncate(40);
    pbs_tasks.sort_by(|a, b| a.age_min.cmp(&b.age_min));
    pbs_tasks.truncate(30);
    let mut coverage = PbsCoverage::default();
    let stale_after_min = 36 * 60;
    if pbs_has_data {
        for g in &all_guests {
            let backup_type = if g.kind == "lxc" { "ct" } else { "vm" }.to_string();
            let key = (backup_type, g.id.to_string());
            match latest_by_guest.get(&key) {
                Some(snap) => {
                    let age_min = ((now - snap.backup_time).max(0)) / 60;
                    coverage.protected += 1;
                    if age_min > stale_after_min {
                        coverage.stale += 1;
                        let title = format!("{} backup is stale", g.name);
                        coverage.findings.push(PbsCoverageFinding {
                            id: format!("stale:{}:{}", g.kind, g.id),
                            tone: "warn".to_string(),
                            kind: "stale".to_string(),
                            title: title.clone(),
                            detail: format!(
                                "Last PBS backup is {} via namespace {}.",
                                fmt_ago(age_min),
                                if snap.namespace.is_empty() {
                                    "root"
                                } else {
                                    &snap.namespace
                                }
                            ),
                            guest_id: g.id,
                            guest_type: g.kind.clone(),
                            guest_name: g.name.clone(),
                            namespace: snap.namespace.clone(),
                            age_min: Some(age_min),
                        });
                        cands.push(Candidate {
                            key: format!("pbs:backup:{}:{}:stale", g.kind, g.id),
                            sev: "warn".to_string(),
                            source: "pbs".to_string(),
                            host: g.server.clone(),
                            target: format!("{} {} · {}", g.kind.to_uppercase(), g.id, g.name),
                            title,
                            desc: format!(
                                "{} has not had a fresh PBS backup in {}.",
                                g.name,
                                fmt_ago(age_min)
                            ),
                            rule: "pbs.backup.age > 36h".to_string(),
                        });
                    }
                }
                None => {
                    coverage.missing += 1;
                    let title = format!("{} has no PBS backup", g.name);
                    coverage.findings.push(PbsCoverageFinding {
                        id: format!("missing:{}:{}", g.kind, g.id),
                        tone: "crit".to_string(),
                        kind: "missing".to_string(),
                        title: title.clone(),
                        detail: format!(
                            "No PBS backup group matched {} {} from {} / {}.",
                            g.kind.to_uppercase(),
                            g.id,
                            g.server,
                            g.node
                        ),
                        guest_id: g.id,
                        guest_type: g.kind.clone(),
                        guest_name: g.name.clone(),
                        namespace: String::new(),
                        age_min: None,
                    });
                    cands.push(Candidate {
                        key: format!("pbs:backup:{}:{}:missing", g.kind, g.id),
                        sev: "warn".to_string(),
                        source: "pbs".to_string(),
                        host: g.server.clone(),
                        target: format!("{} {} · {}", g.kind.to_uppercase(), g.id, g.name),
                        title,
                        desc: "Sentinel could not find a matching backup group in PBS.".to_string(),
                        rule: "pbs.coverage.missing".to_string(),
                    });
                }
            }
        }
        for (backup_type, backup_id) in backup_keys {
            let guest_kind = if backup_type == "ct" { "lxc" } else { "vm" };
            let vmid = backup_id.parse::<u64>().unwrap_or(0);
            if !all_guests
                .iter()
                .any(|g| g.kind == guest_kind && g.id == vmid)
            {
                coverage.orphaned += 1;
            }
        }
        coverage
            .findings
            .sort_by(|a, b| a.kind.cmp(&b.kind).then(a.guest_id.cmp(&b.guest_id)));
        coverage.findings.truncate(40);
    }
    let latest_backup_age_min = pbs_backups.first().map(|b| b.age_min);
    let max_pbs_usage = pbs_datastores.iter().map(|d| d.used_pct).max().unwrap_or(0);
    let pbs_kpis = vec![
        kpi(
            pbs_backups.len().to_string(),
            "",
            "recent PBS snapshots indexed".to_string(),
            0.0,
            vec![],
        ),
        kpi(
            latest_backup_age_min
                .map(|m| fmt_ago(m))
                .unwrap_or_else(|| "never".to_string()),
            "",
            "latest successful backup".to_string(),
            0.0,
            vec![],
        ),
        kpi(
            max_pbs_usage.to_string(),
            "%",
            "highest datastore usage".to_string(),
            0.0,
            vec![],
        ),
        kpi(
            (coverage.missing + coverage.stale).to_string(),
            "",
            format!("{} missing · {} stale", coverage.missing, coverage.stale),
            0.0,
            vec![],
        ),
    ];
    let bmc_ok = bmc_controllers.iter().filter(|c| c.health.eq_ignore_ascii_case("ok") || c.health == "Unknown").count() as u32;
    let temp_max = bmc_sensors
        .iter()
        .filter(|s| s.kind == "temperature")
        .filter_map(|s| s.reading)
        .fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |m| m.max(v))));
    let bmc_warn = bmc_sensors.iter().filter(|s| s.tone != "ok").count() as u32
        + bmc_drives.iter().filter(|d| d.tone != "ok").count() as u32;
    let bmc_view = BmcView {
        kpis: vec![
            kpi(bmc_ok.to_string(), format!("/{}", bmc_controllers.len()), "BMCs healthy", 0.0, vec![]),
            kpi(temp_max.map(|v| v.round().to_string()).unwrap_or_else(|| "—".to_string()), if temp_max.is_some() { "°C".to_string() } else { "".to_string() }, "max temperature", 0.0, vec![]),
            kpi(bmc_sensors.len().to_string(), "".to_string(), "IPMI sensor readings", 0.0, vec![]),
            kpi(bmc_warn.to_string(), "".to_string(), "warnings", 0.0, vec![]),
        ],
        controllers: bmc_controllers,
        sensors: bmc_sensors,
        drives: bmc_drives,
    };

    let pbs_view = PbsView {
        kpis: pbs_kpis,
        datastores: pbs_datastores,
        recent_backups: pbs_backups,
        recent_tasks: pbs_tasks,
        coverage,
        latest_backup_age_min,
        failed_tasks_24h,
    };

    // ── Cluster-wide scalars ─────────────────────────────────────────────
    let nodes_online = nodes.iter().filter(|n| n.status == "ok").count() as u32;
    let nodes_total = nodes.len() as u32;
    let devices_online = unifi_view
        .devices
        .iter()
        .filter(|d| d.status != "crit")
        .count() as u32;
    let devices_total = unifi_view.devices.len() as u32;
    let hosts_up = nodes_online + devices_online + unraid_servers_online;
    let hosts_total = nodes_total + devices_total + unraid_servers_total;
    let availability = if hosts_total == 0 {
        100.0
    } else {
        hosts_up as f64 / hosts_total as f64 * 100.0
    };
    let storage_tb_used = storage_used as f64 / 1_099_511_627_776.0;
    let storage_tb_total = storage_total as f64 / 1_099_511_627_776.0;

    // ── Alerts ───────────────────────────────────────────────────────────
    cands.sort_by_key(|a| sev_rank(&a.sev));
    let reconciled = store.reconcile(&cands, now);
    let notifications = reconciled.newly_active.clone();
    events.extend(reconciled.events);
    let mut alerts = reconciled.alerts;
    alerts.sort_by(|a, b| {
        sev_rank(&a.sev)
            .cmp(&sev_rank(&b.sev))
            .then(a.age_min.cmp(&b.age_min))
    });
    let active = alerts.iter().filter(|a| a.status == "open").count() as u32;
    let crit = alerts
        .iter()
        .filter(|a| a.sev == "crit" && a.status == "open")
        .count() as u32;
    let warn = alerts
        .iter()
        .filter(|a| a.sev == "warn" && a.status == "open")
        .count() as u32;

    // ── Events ───────────────────────────────────────────────────────────
    events.sort_by(|a, b| b.ts.cmp(&a.ts));
    events.truncate(220);
    let error_events = events.iter().filter(|e| e.level == "error").count() as u32;

    let vm_count = all_guests.iter().filter(|g| g.kind == "vm").count() as u32;
    let lxc_count = all_guests.iter().filter(|g| g.kind == "lxc").count() as u32;

    // ── Append history sample, then derive sparklines from it ────────────
    history.set_max(config.history_max_samples);
    let sample = Sample {
        t: now,
        wan_down_mbps: wan_down,
        wan_up_mbps: wan_up,
        availability,
        devices_online: devices_online as f64,
        devices_total: devices_total as f64,
        active_alerts: active as f64,
        alerts_crit: crit as f64,
        alerts_warn: warn as f64,
        vm_count: vm_count as f64,
        lxc_count: lxc_count as f64,
        nodes_online: nodes_online as f64,
        storage_tb: storage_tb_used,
        wireless_clients: unifi_view.wireless_clients as f64,
        wired_clients: unifi_view.wired_clients as f64,
        poe_ports: unifi_view.poe_active as f64,
        unraid_servers_online: unraid_servers_online as f64,
        unraid_array_used_pct: unraid_storage_used_pct,
        unraid_array_used_tb: unraid_storage_used_tb,
        unraid_containers_running: unraid_containers_running as f64,
        unraid_vms_running: unraid_vms_running as f64,
        events_total: events.len() as f64,
        error_events: error_events as f64,
    };
    history.push(sample.clone());

    // ── Dashboard ────────────────────────────────────────────────────────
    let bandwidth = build_bandwidth(history);
    let dash_kpis = vec![
        kpi(
            format!("{availability:.1}"),
            "%",
            format!("{hosts_up} of {hosts_total} hosts online"),
            history.trend(24, |s| s.availability),
            history.spark(24, |s| s.availability),
        ),
        kpi(
            devices_online.to_string(),
            format!("/{devices_total}"),
            format!(
                "{} online · {} offline",
                devices_online,
                devices_total.saturating_sub(devices_online)
            ),
            history.trend(24, |s| s.devices_online),
            history.spark(24, |s| s.devices_online),
        ),
        kpi(
            active.to_string(),
            "",
            format!("{crit} critical · {warn} warning"),
            history.trend(24, |s| s.active_alerts),
            history.spark(24, |s| s.active_alerts),
        ),
        {
            let total = wan_down + wan_up;
            let (disp, unit) = if total >= 1000.0 {
                (format!("{:.2}", total / 1000.0), "Gbps")
            } else {
                (format!("{total:.0}"), "Mbps")
            };
            let wan_trend = history.trend(24, |s| s.wan_down_mbps + s.wan_up_mbps);
            kpi(
                disp,
                unit,
                format!("↓ {} · ↑ {}", fmt_mbps(wan_down), fmt_mbps(wan_up)),
                if total >= 1000.0 {
                    wan_trend / 1000.0
                } else {
                    wan_trend
                },
                history.spark(24, |s| s.wan_down_mbps + s.wan_up_mbps),
            )
        },
    ];

    let total_guests = all_guests.len() as u32;
    let issues = build_dashboard_issues(&alerts);

    let dashboard = Dashboard {
        kpis: dash_kpis,
        issues,
        bandwidth,
        nodes: nodes.clone(),
        topology_counts: count_topology(&topology),
        total_guests,
        quorum: if quorum_labels.is_empty() {
            None
        } else {
            Some(quorum_labels.join(" · "))
        },
    };

    // ── Proxmox page ─────────────────────────────────────────────────────
    let running = all_guests.iter().filter(|g| g.status != "stop").count() as u32;
    let stopped = total_guests - running;
    let high_cpu = all_guests.iter().filter(|g| g.cpu >= 80).count() as u32;
    let high_mem = all_guests.iter().filter(|g| g.mem >= 80).count() as u32;
    let running_vm = all_guests
        .iter()
        .filter(|g| g.kind == "vm" && g.status != "stop")
        .count() as u32;
    let running_lxc = all_guests
        .iter()
        .filter(|g| g.kind == "lxc" && g.status != "stop")
        .count() as u32;
    let proxmox_kpis = vec![
        kpi(
            format!("{nodes_online} / {nodes_total}"),
            "",
            format!("across {} server(s)", pmx.len()),
            history.trend(24, |s| s.nodes_online),
            history.spark(24, |s| s.nodes_online),
        ),
        kpi(
            vm_count.to_string(),
            "",
            format!("{running_vm} running"),
            history.trend(24, |s| s.vm_count),
            history.spark(24, |s| s.vm_count),
        ),
        kpi(
            lxc_count.to_string(),
            "",
            format!(
                "{running_lxc} running · {} stopped",
                lxc_count.saturating_sub(running_lxc)
            ),
            history.trend(24, |s| s.lxc_count),
            history.spark(24, |s| s.lxc_count),
        ),
        kpi(
            format!("{storage_tb_used:.1}"),
            format!("/ {storage_tb_total:.0} TB"),
            format!(
                "{}% of guest storage used",
                pct(storage_used, storage_total)
            ),
            history.trend(24, |s| s.storage_tb),
            history.spark(24, |s| s.storage_tb),
        ),
    ];
    let proxmox = ProxmoxView {
        kpis: proxmox_kpis,
        nodes,
        guests: node_guests,
        high_cpu,
        high_mem,
        running,
        stopped,
    };

    // ── UniFi page KPIs ──────────────────────────────────────────────────
    let ap_count = unifi_view
        .devices
        .iter()
        .filter(|d| d.kind == "Access Point")
        .count() as u32;
    let unifi_kpis = vec![
        kpi(
            devices_total.to_string(),
            "",
            format!(
                "{} online · {} offline",
                devices_online,
                devices_total.saturating_sub(devices_online)
            ),
            history.trend(24, |s| s.devices_online),
            history.spark(24, |s| s.devices_online),
        ),
        kpi(
            unifi_view.wireless_clients.to_string(),
            "",
            format!("across {ap_count} access points"),
            history.trend(24, |s| s.wireless_clients),
            history.spark(24, |s| s.wireless_clients),
        ),
        kpi(
            unifi_view.poe_active.to_string(),
            format!("/{}", unifi_view.poe_capable),
            format!(
                "{}% of PoE ports delivering",
                pct(
                    unifi_view.poe_active as u64,
                    unifi_view.poe_capable.max(1) as u64
                )
            ),
            history.trend(24, |s| s.poe_ports),
            history.spark(24, |s| s.poe_ports),
        ),
        {
            let total = wan_down + wan_up;
            let (disp, unit) = if total >= 1000.0 {
                (format!("{:.2}", total / 1000.0), "Gbps")
            } else {
                (format!("{total:.0}"), "Mbps")
            };
            let wan_trend = history.trend(24, |s| s.wan_down_mbps + s.wan_up_mbps);
            kpi(
                disp,
                unit,
                format!("↓ {} · ↑ {}", fmt_mbps(wan_down), fmt_mbps(wan_up)),
                if total >= 1000.0 {
                    wan_trend / 1000.0
                } else {
                    wan_trend
                },
                history.spark(24, |s| s.wan_down_mbps + s.wan_up_mbps),
            )
        },
    ];
    let unifi_view = UnifiView {
        kpis: unifi_kpis,
        ..unifi_view
    };

    // ── Unraid page KPIs ─────────────────────────────────────────────────
    let unraid_kpis = vec![
        kpi(
            format!("{unraid_servers_online} / {unraid_servers_total}"),
            "",
            "servers online".to_string(),
            history.trend(24, |s| s.unraid_servers_online),
            history.spark(24, |s| s.unraid_servers_online),
        ),
        kpi(
            format!("{unraid_storage_used_pct:.0}"),
            "%",
            format!("{unraid_storage_warn} server(s) above threshold"),
            history.trend(24, |s| s.unraid_array_used_pct),
            history.spark(24, |s| s.unraid_array_used_pct),
        ),
        kpi(
            unraid_containers_running.to_string(),
            format!("/{}", unraid_containers_total),
            "Docker containers running".to_string(),
            history.trend(24, |s| s.unraid_containers_running),
            history.spark(24, |s| s.unraid_containers_running),
        ),
        kpi(
            unraid_vms_running.to_string(),
            format!("/{}", unraid_vms_total),
            "VMs running".to_string(),
            history.trend(24, |s| s.unraid_vms_running),
            history.spark(24, |s| s.unraid_vms_running),
        ),
    ];
    let unraid = UnraidView {
        kpis: unraid_kpis,
        servers: unraid_servers,
        containers_running: unraid_containers_running,
        containers_total: unraid_containers_total,
        vms_running: unraid_vms_running,
        vms_total: unraid_vms_total,
        array_warn: unraid_storage_warn,
        software_update_count: unraid_software_update_count,
    };

    // ── Alerts + Events views ────────────────────────────────────────────
    let alerts_view = build_alerts_view(alerts, history);
    let events_view = build_events_view(events, history);

    let snapshot = Snapshot {
        generated_at: Utc::now().to_rfc3339(),
        poll_interval_sec: config.poll_interval_sec,
        sources,
        dashboard,
        proxmox,
        unifi: unifi_view,
        unraid,
        pbs: pbs_view,
        bmc: bmc_view,
        topology,
        alerts: alerts_view,
        events: events_view,
    };
    (snapshot, sample, notifications)
}

fn proxmox_quorum_label(data: &ProxmoxData) -> Option<String> {
    let cluster = data.cluster_status.iter().find(|s| s.kind == "cluster")?;
    let status_nodes: Vec<_> = data
        .cluster_status
        .iter()
        .filter(|s| s.kind == "node")
        .collect();
    let resource_nodes_total = data.resources.iter().filter(|r| r.kind == "node").count() as u32;
    let total = cluster
        .nodes
        .unwrap_or(status_nodes.len() as u32)
        .max(resource_nodes_total);
    if total < 2 {
        return None;
    }

    let status_has_online = status_nodes.iter().any(|s| s.online.is_some());
    let status_online = status_nodes
        .iter()
        .filter(|s| s.online.unwrap_or(0) != 0)
        .count() as u32;
    let resource_online = data
        .resources
        .iter()
        .filter(|r| r.kind == "node" && r.status.as_deref() == Some("online"))
        .count() as u32;
    let online = if status_has_online {
        status_online
    } else {
        resource_online
    };

    Some(format!("{online}/{total}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AlertThresholds, UiPrefs};
    use crate::network_scanner::NetworkScannerSettings;
    use crate::notify::NotificationSettings;
    use crate::pbs::{PbsDatastore, PbsGroup, PbsSnapshot};
    use crate::proxmox::{PveClusterStatus, PveResource};

    fn runtime_config() -> RuntimeConfig {
        RuntimeConfig {
            poll_interval_sec: 15,
            bind: "127.0.0.1:0".to_string(),
            http_timeout_sec: 12,
            history_max_samples: 100,
            history_retention_days: 30,
            frontend_poll_ms: 5000,
            thresholds: AlertThresholds::default(),
            ui_prefs: UiPrefs::default(),
            notifications: NotificationSettings::default(),
            network_scanner: NetworkScannerSettings::default(),
            unifi: None,
            proxmox: Vec::new(),
            pbs: Vec::new(),
            bmc: Vec::new(),
            unraid: Vec::new(),
        }
    }

    fn node(name: &str, status: &str) -> PveResource {
        PveResource {
            kind: "node".to_string(),
            node: Some(name.to_string()),
            status: Some(status.to_string()),
            name: None,
            storage: None,
            content: None,
            tags: None,
            vmid: None,
            cpu: 0.0,
            maxcpu: 0.0,
            mem: 0,
            maxmem: 0,
            disk: 0,
            maxdisk: 0,
            uptime: 0,
        }
    }

    fn guest(kind: &str, vmid: u64, name: &str) -> PveResource {
        PveResource {
            kind: kind.to_string(),
            node: Some("pve1".to_string()),
            status: Some("running".to_string()),
            name: Some(name.to_string()),
            storage: None,
            content: None,
            tags: None,
            vmid: Some(vmid),
            cpu: 0.0,
            maxcpu: 2.0,
            mem: 1,
            maxmem: 100,
            disk: 1,
            maxdisk: 100,
            uptime: 60,
        }
    }

    fn cluster_row(nodes: u32) -> PveClusterStatus {
        PveClusterStatus {
            kind: "cluster".to_string(),
            id: Some("cluster".to_string()),
            name: Some("pve".to_string()),
            nodes: Some(nodes),
            quorate: Some(1),
            online: None,
        }
    }

    fn status_node(name: &str, online: u8) -> PveClusterStatus {
        PveClusterStatus {
            kind: "node".to_string(),
            id: Some(format!("node/{name}")),
            name: Some(name.to_string()),
            nodes: None,
            quorate: None,
            online: Some(online),
        }
    }

    fn data(cluster_status: Vec<PveClusterStatus>) -> ProxmoxData {
        ProxmoxData {
            server: "pve".to_string(),
            release: "8.2".to_string(),
            resources: vec![node("pve1", "online"), node("pve2", "online")],
            cluster_status,
            node_rrd: Default::default(),
            tasks: Vec::new(),
        }
    }

    fn pbs_datastore(store: &str) -> PbsDatastore {
        PbsDatastore {
            store: store.to_string(),
            used: Some(10),
            total: Some(100),
            avail: Some(90),
            gc_status: None,
            maintenance_mode: None,
        }
    }

    fn pbs_group(store: &str, backup_id: &str) -> PbsGroup {
        PbsGroup {
            datastore: store.to_string(),
            namespace: String::new(),
            backup_type: "vm".to_string(),
            backup_id: backup_id.to_string(),
            backup_count: 1,
            last_backup: None,
            owner: None,
        }
    }

    fn pbs_snapshot(store: &str, backup_id: &str, backup_time: i64) -> PbsSnapshot {
        PbsSnapshot {
            datastore: store.to_string(),
            namespace: String::new(),
            backup_type: "vm".to_string(),
            backup_id: backup_id.to_string(),
            backup_time,
            comment: None,
            owner: None,
            protected: false,
            size: Some(1),
            verification: None,
        }
    }

    fn render(
        pmx: &[(String, Result<ProxmoxData, String>)],
        pbs: &[(String, Result<PbsData, String>)],
    ) -> Snapshot {
        let cfg = runtime_config();
        let mut history = History::new(Vec::new(), 100);
        let mut store = AlertStore::default();
        build(&cfg, pmx, pbs, &[], &[], &None, &mut history, &mut store).0
    }

    #[test]
    fn standalone_proxmox_has_no_quorum_label() {
        let d = data(vec![status_node("pve1", 1)]);

        assert_eq!(proxmox_quorum_label(&d), None);
    }

    #[test]
    fn clustered_proxmox_has_quorum_label() {
        let d = data(vec![
            cluster_row(2),
            status_node("pve1", 1),
            status_node("pve2", 1),
        ]);

        assert_eq!(proxmox_quorum_label(&d), Some("2/2".to_string()));
    }

    #[test]
    fn pbs_coverage_is_suppressed_without_usable_pbs_data() {
        let mut pmx_data = data(vec![status_node("pve1", 1)]);
        pmx_data.resources.push(guest("qemu", 101, "app"));
        let snapshot = render(
            &[("pve".to_string(), Ok(pmx_data))],
            &[("blackbox".to_string(), Err("offline".to_string()))],
        );

        assert_eq!(snapshot.pbs.coverage.missing, 0);
        assert!(snapshot.pbs.coverage.findings.is_empty());
        assert!(snapshot
            .alerts
            .alerts
            .iter()
            .all(|a| a.rule != "pbs.coverage.missing"));
    }

    #[test]
    fn pbs_view_merges_sources_before_truncating_and_counts_per_datastore() {
        let now = Utc::now().timestamp();
        let mut source_a_snapshots = (0..80)
            .map(|i| pbs_snapshot("store-a", &format!("{}", 1000 + i), now - 10_000 - i))
            .collect::<Vec<_>>();
        source_a_snapshots.push(pbs_snapshot("store-b", "3000", now - 20_000));
        let source_a = PbsData {
            server: "pbs-a".to_string(),
            version: "4.0".to_string(),
            datastores: vec![pbs_datastore("store-a"), pbs_datastore("store-b")],
            groups: vec![
                pbs_group("store-a", "1000"),
                pbs_group("store-a", "1001"),
                pbs_group("store-b", "3000"),
            ],
            snapshots: source_a_snapshots,
            tasks: Vec::new(),
        };
        let source_b = PbsData {
            server: "pbs-b".to_string(),
            version: "4.0".to_string(),
            datastores: vec![pbs_datastore("store-c")],
            groups: vec![pbs_group("store-c", "4000")],
            snapshots: vec![pbs_snapshot("store-c", "4000", now)],
            tasks: Vec::new(),
        };
        let snapshot = render(
            &[],
            &[
                ("pbs-a".to_string(), Ok(source_a)),
                ("pbs-b".to_string(), Ok(source_b)),
            ],
        );

        assert_eq!(snapshot.pbs.recent_backups.len(), 40);
        assert_eq!(snapshot.pbs.recent_backups[0].source, "pbs-b");
        let store_a = snapshot
            .pbs
            .datastores
            .iter()
            .find(|d| d.source == "pbs-a" && d.store == "store-a")
            .unwrap();
        let store_b = snapshot
            .pbs
            .datastores
            .iter()
            .find(|d| d.source == "pbs-a" && d.store == "store-b")
            .unwrap();
        assert_eq!(store_a.groups, 2);
        assert_eq!(store_a.snapshots, 80);
        assert_eq!(store_b.groups, 1);
        assert_eq!(store_b.snapshots, 1);
    }
}
