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

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use chrono::Utc;
use sqlx::PgPool;

pub use alerts::{patch_alerts, AlertStore};

use self::alerts::{build_alerts_view, sev_rank, Candidate};
use self::bandwidth::build_bandwidth;
use self::events::build_events_view;
use self::format::{fmt_ago, fmt_mbps, kpi, pct};
use self::proxmox_view::process_proxmox;
use self::topology::count_topology;
use self::unifi_view::process_unifi;
use self::unraid_view::process_unraid;
use crate::config::RuntimeConfig;
use crate::db;
use crate::history::{History, Sample};
use crate::model::*;
use crate::proxmox::{ProxmoxClient, ProxmoxData};
use crate::unifi::{UnifiClient, UnifiData};
use crate::unraid::{UnraidClient, UnraidData};

/// The HTTP clients built from the current source set.
pub struct Clients {
    pub proxmox: Vec<ProxmoxClient>,
    pub unraid: Vec<UnraidClient>,
    pub unifi: Option<UnifiClient>,
    /// `RuntimeConfig::source_sig` of the config these were built from.
    pub source_sig: u64,
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
    pub snapshot: RwLock<Arc<Snapshot>>,
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
                "sources changed — {} Proxmox client(s), {} Unraid client(s), UniFi {}",
                clients.proxmox.len(),
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
    let (proxmox, unraid, unifi) = {
        let c = state.clients.read().unwrap();
        (c.proxmox.clone(), c.unraid.clone(), c.unifi.clone())
    };

    // Query Proxmox hosts and UniFi concurrently.
    let pmx_futs = proxmox.iter().map(|c| async move {
        (
            c.name.clone(),
            c.collect().await.map_err(|e| format!("{e:#}")),
        )
    });
    let unraid_futs = unraid.iter().map(|c| async move {
        (
            c.name.clone(),
            c.collect().await.map_err(|e| format!("{e:#}")),
        )
    });
    let unifi_fut = async {
        match &unifi {
            Some(c) => Some(c.collect().await.map_err(|e| format!("{e:#}"))),
            None => None,
        }
    };
    let (pmx, unraid_res, unifi_res) = futures::join!(
        futures::future::join_all(pmx_futs),
        futures::future::join_all(unraid_futs),
        unifi_fut
    );

    let (snapshot, sample) = {
        let mut history = state.history.write().unwrap();
        let mut store = state.alerts.write().unwrap();
        build(cfg, &pmx, &unraid_res, &unifi_res, &mut history, &mut store)
    };

    // Persist the new sample to the time-series table and the reconciled alert
    // workflow state; failures are logged but never fatal.
    if let Err(e) = db::insert_sample(&state.pool, &sample).await {
        tracing::warn!("could not persist metric sample: {e:#}");
    }
    let alert_rows = state.alerts.read().unwrap().rows();
    if let Err(e) = db::save_alert_state(&state.pool, &alert_rows).await {
        tracing::warn!("could not persist alert state: {e:#}");
    }

    let ok_sources = snapshot.sources.iter().filter(|s| s.ok).count();
    tracing::info!(
        "poll complete — {ok_sources}/{} sources up, {} alerts, {} events",
        snapshot.sources.len(),
        snapshot.alerts.alerts.len(),
        snapshot.events.events.len(),
    );

    *state.snapshot.write().unwrap() = Arc::new(snapshot);
}

fn build(
    config: &RuntimeConfig,
    pmx: &[(String, Result<ProxmoxData, String>)],
    unraid: &[(String, Result<UnraidData, String>)],
    unifi: &Option<Result<UnifiData, String>>,
    history: &mut History,
    store: &mut AlertStore,
) -> (Snapshot, Sample) {
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

    for (name, result) in pmx {
        match result {
            Err(e) => sources.push(SourceHealth {
                name: name.clone(),
                kind: "proxmox".to_string(),
                ok: false,
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
                sources.push(SourceHealth {
                    name: name.clone(),
                    kind: "proxmox".to_string(),
                    ok: true,
                    detail: format!("PVE {} · {} node(s)", data.release, node_count),
                    error: None,
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
    let mut unraid_array_used_pct_sum = 0.0;
    let mut unraid_array_used_tb = 0.0;
    let mut unraid_array_warn = 0u32;
    let mut unraid_software_update_count = 0u32;
    for (name, result) in unraid {
        match result {
            Err(e) => sources.push(SourceHealth {
                name: name.clone(),
                kind: "unraid".to_string(),
                ok: false,
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
                unraid_array_used_pct_sum += processed.array_used_pct as f64;
                unraid_array_used_tb += processed.array_used_tb;
                if processed.array_used_pct >= config.thresholds.unraid_array_warn {
                    unraid_array_warn += 1;
                }
                events.extend(processed.events);
                cands.extend(processed.candidates);
                sources.push(SourceHealth {
                    name: data.source_name.clone(),
                    kind: "unraid".to_string(),
                    ok: true,
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
    let unraid_array_used_pct = if unraid_servers_total == 0 {
        0.0
    } else {
        unraid_array_used_pct_sum / unraid_servers_total as f64
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
    cands.sort_by(|a, b| sev_rank(&a.sev).cmp(&sev_rank(&b.sev)));
    let mut alerts = store.reconcile(&cands, now);
    alerts.sort_by(|a, b| {
        sev_rank(&a.sev)
            .cmp(&sev_rank(&b.sev))
            .then(a.age_min.cmp(&b.age_min))
    });
    let active = alerts.iter().filter(|a| a.status != "resolved").count() as u32;
    let crit = alerts
        .iter()
        .filter(|a| a.sev == "crit" && a.status != "resolved")
        .count() as u32;
    let warn = alerts
        .iter()
        .filter(|a| a.sev == "warn" && a.status != "resolved")
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
        unraid_array_used_pct,
        unraid_array_used_tb,
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
    let issues: Vec<Issue> = alerts
        .iter()
        .filter(|a| a.status != "resolved")
        .take(6)
        .map(|a| Issue {
            sev: a.sev.clone(),
            title: a.title.clone(),
            source: format!("{} · {}", a.host, a.target),
            time: fmt_ago(a.age_min),
        })
        .collect();

    let dashboard = Dashboard {
        kpis: dash_kpis,
        issues,
        bandwidth,
        nodes: nodes.clone(),
        topology_counts: count_topology(&topology),
        total_guests,
        quorum: format!("{nodes_online}/{nodes_total}"),
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
            format!("{unraid_array_used_pct:.0}"),
            "%",
            format!("{unraid_array_warn} server(s) above threshold"),
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
        array_warn: unraid_array_warn,
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
        topology,
        alerts: alerts_view,
        events: events_view,
    };
    (snapshot, sample)
}
