//! Polling engine — turns raw Proxmox/UniFi data into the served [`Snapshot`].
//!
//! A background task polls every source on a fixed interval, builds the full
//! snapshot (KPIs, nodes, guests, devices, topology, alerts, events) and stores
//! it behind an `RwLock`. HTTP handlers only ever read the latest snapshot.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, Utc};
use sqlx::PgPool;

use crate::config::{AlertThresholds, RuntimeConfig};
use crate::db::{self, AlertStateRow};
use crate::history::{History, Sample};
use crate::model::*;
use crate::proxmox::{ProxmoxClient, ProxmoxData};
use crate::unifi::{UnifiClient, UnifiData};

/// The HTTP clients built from the current source set.
pub struct Clients {
    pub proxmox: Vec<ProxmoxClient>,
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

// ── Alert state tracking ────────────────────────────────────────────────────

/// Per-alert bookkeeping: first-seen time, re-fire count and workflow status.
struct AlertMeta {
    first_seen: i64,
    last_seen: i64,
    occurrences: u32,
    status: String,
    assignee: Option<String>,
    present: bool,
    present_before: bool,
}

#[derive(Default)]
pub struct AlertStore {
    map: HashMap<String, AlertMeta>,
}

/// A condition that currently holds and should surface as an alert.
struct Candidate {
    key: String,
    sev: String,
    source: String,
    host: String,
    target: String,
    title: String,
    desc: String,
    rule: String,
}

impl AlertStore {
    /// Reconcile freshly-derived candidates with tracked state, returning the
    /// alert list with stable ages, re-fire counts and workflow statuses.
    fn reconcile(&mut self, cands: &[Candidate], now: i64) -> Vec<Alert> {
        for m in self.map.values_mut() {
            m.present_before = m.present;
            m.present = false;
        }
        let mut out = Vec::new();
        for c in cands {
            let meta = self.map.entry(c.key.clone()).or_insert(AlertMeta {
                first_seen: now,
                last_seen: now,
                occurrences: 0,
                status: "open".to_string(),
                assignee: None,
                present: false,
                present_before: false,
            });
            if !meta.present_before {
                meta.occurrences += 1;
                if meta.status == "resolved" {
                    meta.status = "open".to_string();
                    meta.assignee = None;
                }
            }
            meta.present = true;
            meta.last_seen = now;
            out.push(Alert {
                id: c.key.clone(),
                sev: c.sev.clone(),
                status: meta.status.clone(),
                title: c.title.clone(),
                desc: c.desc.clone(),
                source: c.source.clone(),
                host: c.host.clone(),
                target: c.target.clone(),
                age_min: (now - meta.first_seen) / 60,
                occurrences: meta.occurrences,
                assignee: meta.assignee.clone(),
                rule: c.rule.clone(),
            });
        }
        // Drop conditions that cleared more than 30 minutes ago.
        self.map.retain(|_, m| m.present || now - m.last_seen < 1800);
        out
    }

    /// Current workflow status and assignee for an alert, if tracked.
    pub fn status_of(&self, id: &str) -> Option<(String, Option<String>)> {
        self.map
            .get(id)
            .map(|m| (m.status.clone(), m.assignee.clone()))
    }

    /// Apply a workflow action to one alert; returns `true` if it existed.
    pub fn apply(&mut self, id: &str, action: &str) -> bool {
        let Some(meta) = self.map.get_mut(id) else {
            return false;
        };
        match action {
            "ack" => {
                meta.status = "ack".to_string();
                meta.assignee = Some("J. Pals".to_string());
            }
            "resolve" => meta.status = "resolved".to_string(),
            "reopen" => {
                meta.status = "open".to_string();
                meta.assignee = None;
            }
            _ => return false,
        }
        true
    }

    /// Rebuild the store from persisted rows — restores ack/resolve state and
    /// alert ages across a backend restart.
    pub fn from_rows(rows: Vec<AlertStateRow>) -> Self {
        let mut map = HashMap::new();
        for r in rows {
            map.insert(
                r.alert_key,
                AlertMeta {
                    first_seen: r.first_seen,
                    last_seen: r.last_seen,
                    occurrences: r.occurrences.max(0) as u32,
                    status: r.status,
                    assignee: r.assignee,
                    present: false,
                    present_before: false,
                },
            );
        }
        Self { map }
    }

    /// The tracked state as rows ready to persist to `alert_state`.
    pub fn rows(&self) -> Vec<AlertStateRow> {
        self.map
            .iter()
            .map(|(k, m)| AlertStateRow {
                alert_key: k.clone(),
                first_seen: m.first_seen,
                last_seen: m.last_seen,
                occurrences: m.occurrences as i32,
                status: m.status.clone(),
                assignee: m.assignee.clone(),
            })
            .collect()
    }
}

// ── Formatting helpers ──────────────────────────────────────────────────────

fn pct(used: u64, total: u64) -> u32 {
    if total == 0 {
        0
    } else {
        ((used as f64 / total as f64) * 100.0).round().clamp(0.0, 100.0) as u32
    }
}

fn fmt_uptime(secs: u64) -> String {
    if secs == 0 {
        return "—".to_string();
    }
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}d {h:02}h")
    } else if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m")
    }
}

fn fmt_mem(bytes: u64) -> String {
    let gb = bytes as f64 / 1_073_741_824.0;
    if gb >= 1.0 {
        if (gb.fract()).abs() < 0.05 {
            format!("{} GB", gb.round() as u64)
        } else {
            format!("{gb:.1} GB")
        }
    } else {
        format!("{} MB", bytes / 1_048_576)
    }
}

fn fmt_mbps(mbps: f64) -> String {
    if mbps >= 1000.0 {
        format!("{:.2} Gbps", mbps / 1000.0)
    } else {
        format!("{mbps:.0} Mbps")
    }
}

fn fmt_ago(min: i64) -> String {
    if min < 1 {
        "just now".to_string()
    } else if min < 60 {
        format!("{min}m ago")
    } else if min < 1440 {
        format!("{}h {}m ago", min / 60, min % 60)
    } else {
        format!("{}d ago", min / 1440)
    }
}

fn friendly_task(kind: &str) -> &'static str {
    match kind {
        "vzdump" => "Backup",
        "aptupdate" => "APT update",
        "vncproxy" | "spiceproxy" | "termproxy" => "Console session",
        "vzstart" => "Container start",
        "vzstop" | "vzshutdown" => "Container stop",
        "vzrestart" => "Container restart",
        "qmstart" => "VM start",
        "qmstop" | "qmshutdown" => "VM stop",
        "qmreboot" => "VM reboot",
        "qmigrate" => "VM migration",
        "qmsnapshot" => "VM snapshot",
        "imgcopy" | "imgdel" => "Disk image task",
        "download" => "Download",
        "startall" => "Bulk start",
        "stopall" => "Bulk stop",
        "srvreload" => "Service reload",
        _ => "Task",
    }
}

fn kpi(display: impl Into<String>, unit: impl Into<String>, sub: impl Into<String>, trend: f64, spark: Vec<f64>) -> Kpi {
    Kpi {
        display: display.into(),
        unit: unit.into(),
        sub: sub.into(),
        trend: (trend * 100.0).round() / 100.0,
        spark,
    }
}

// ── Poller ──────────────────────────────────────────────────────────────────

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
                "sources changed — {} Proxmox client(s), UniFi {}",
                clients.proxmox.len(),
                if clients.unifi.is_some() { "configured" } else { "not configured" },
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
    let (proxmox, unifi) = {
        let c = state.clients.read().unwrap();
        (c.proxmox.clone(), c.unifi.clone())
    };

    // Query Proxmox hosts and UniFi concurrently.
    let pmx_futs = proxmox.iter().map(|c| async move {
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
    let (pmx, unifi_res) = futures::join!(futures::future::join_all(pmx_futs), unifi_fut);

    let (snapshot, sample) = {
        let mut history = state.history.write().unwrap();
        let mut store = state.alerts.write().unwrap();
        build(cfg, &pmx, &unifi_res, &mut history, &mut store)
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

// ── Snapshot builder ────────────────────────────────────────────────────────

fn build(
    config: &RuntimeConfig,
    pmx: &[(String, Result<ProxmoxData, String>)],
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
                let (n, g, ev, c, su, st) = process_proxmox(data, now, &config.thresholds);
                let node_count = n.len();
                nodes.extend(n);
                for ng in g {
                    all_guests.extend(ng.guests.iter().cloned());
                    node_guests.push(ng);
                }
                events.extend(ev);
                cands.extend(c);
                storage_used += su;
                storage_total += st;
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
            let (view, topo, evs, ucands, down, up) = process_unifi(data, now, &config.thresholds);
            wan_down = down;
            wan_up = up;
            events.extend(evs);
            cands.extend(ucands);
            sources.push(SourceHealth {
                name: "UniFi".to_string(),
                kind: "unifi".to_string(),
                ok: true,
                detail: format!(
                    "Network {} · {} devices",
                    data.app_version,
                    view.devices.len()
                ),
                error: None,
            });
            unifi_view = view;
            topology = topo;
        }
    }

    // ── Cluster-wide scalars ─────────────────────────────────────────────
    let nodes_online = nodes.iter().filter(|n| n.status == "ok").count() as u32;
    let nodes_total = nodes.len() as u32;
    let devices_online = unifi_view.devices.iter().filter(|d| d.status != "crit").count() as u32;
    let devices_total = unifi_view.devices.len() as u32;
    let hosts_up = nodes_online + devices_online;
    let hosts_total = nodes_total + devices_total;
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
                if total >= 1000.0 { wan_trend / 1000.0 } else { wan_trend },
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
            format!("{running_lxc} running · {} stopped", lxc_count.saturating_sub(running_lxc)),
            history.trend(24, |s| s.lxc_count),
            history.spark(24, |s| s.lxc_count),
        ),
        kpi(
            format!("{storage_tb_used:.1}"),
            format!("/ {storage_tb_total:.0} TB"),
            format!("{}% of guest storage used", pct(storage_used, storage_total)),
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
                pct(unifi_view.poe_active as u64, unifi_view.poe_capable.max(1) as u64)
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
                if total >= 1000.0 { wan_trend / 1000.0 } else { wan_trend },
                history.spark(24, |s| s.wan_down_mbps + s.wan_up_mbps),
            )
        },
    ];
    let unifi_view = UnifiView {
        kpis: unifi_kpis,
        ..unifi_view
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
        topology,
        alerts: alerts_view,
        events: events_view,
    };
    (snapshot, sample)
}

/// Re-apply alert workflow statuses to the live snapshot after an action,
/// so the UI reflects an acknowledge/resolve without waiting for the next poll.
pub fn patch_alerts(state: &AppState) {
    let cur = state.current();
    let mut alerts = cur.alerts.alerts.clone();
    {
        let store = state.alerts.read().unwrap();
        for a in &mut alerts {
            if let Some((status, assignee)) = store.status_of(&a.id) {
                a.status = status;
                a.assignee = assignee;
            }
        }
    }
    let history = state.history.read().unwrap();
    let alerts_view = build_alerts_view(alerts, &history);
    drop(history);
    let mut next = (*cur).clone();
    next.alerts = alerts_view;
    *state.snapshot.write().unwrap() = Arc::new(next);
}

fn sev_rank(sev: &str) -> u8 {
    match sev {
        "crit" => 0,
        "warn" => 1,
        _ => 2,
    }
}

// ── Proxmox processing ──────────────────────────────────────────────────────

#[allow(clippy::type_complexity)]
fn process_proxmox(
    data: &ProxmoxData,
    now: i64,
    th: &AlertThresholds,
) -> (
    Vec<NodeTile>,
    Vec<NodeGuests>,
    Vec<Event>,
    Vec<Candidate>,
    u64,
    u64,
) {
    let mut tiles: Vec<NodeTile> = Vec::new();
    let mut guests_by_node: HashMap<String, Vec<Guest>> = HashMap::new();
    let mut events: Vec<Event> = Vec::new();
    let mut cands: Vec<Candidate> = Vec::new();
    let mut storage_used = 0u64;
    let mut storage_total = 0u64;

    // Guests first, so node tiles can count them.
    for r in &data.resources {
        if r.kind != "qemu" && r.kind != "lxc" {
            continue;
        }
        let Some(node) = r.node.clone() else { continue };
        let running = r.status.as_deref() == Some("running");
        let cpu = (r.cpu * 100.0).round() as u32;
        let mem = pct(r.mem, r.maxmem);
        let disk = pct(r.disk, r.maxdisk);
        let status = if !running {
            "stop".to_string()
        } else if mem >= th.guest_mem_crit {
            "crit".to_string()
        } else if mem >= th.guest_mem_warn || cpu >= th.guest_cpu_warn {
            "warn".to_string()
        } else {
            "ok".to_string()
        };
        let tags = r
            .tags
            .as_deref()
            .filter(|t| !t.is_empty())
            .map(|t| t.replace(';', ", "))
            .unwrap_or_default();
        let kind = if r.kind == "qemu" { "vm" } else { "lxc" };
        let guest = Guest {
            id: r.vmid.unwrap_or(0),
            kind: kind.to_string(),
            name: r.name.clone().unwrap_or_else(|| format!("{kind}-{}", r.vmid.unwrap_or(0))),
            status,
            cpu,
            mem,
            disk,
            net: 0,
            uptime: fmt_uptime(r.uptime),
            tags,
            cores: r.maxcpu.round() as u32,
            ram: fmt_mem(r.maxmem),
            node: node.clone(),
            server: data.server.clone(),
        };

        // Derive alerts from guest health.
        if running {
            let label = format!(
                "{} {} · {}",
                if kind == "vm" { "VM" } else { "CT" },
                guest.id,
                guest.name
            );
            if mem >= th.guest_mem_crit {
                cands.push(Candidate {
                    key: format!("pmx:{}:{}:{}:mem", data.server, node, guest.id),
                    sev: "crit".to_string(),
                    source: "proxmox".to_string(),
                    host: format!("{} / {}", data.server, node),
                    target: label.clone(),
                    title: format!("Memory pressure {mem}% on {}", guest.name),
                    desc: format!(
                        "{label} is at {mem}% memory utilization on node {node}. Sustained pressure can trigger swapping and OOM kills."
                    ),
                    rule: format!("guest.mem.utilization >= {}%", th.guest_mem_crit),
                });
            } else if mem >= th.guest_mem_warn {
                cands.push(Candidate {
                    key: format!("pmx:{}:{}:{}:mem", data.server, node, guest.id),
                    sev: "warn".to_string(),
                    source: "proxmox".to_string(),
                    host: format!("{} / {}", data.server, node),
                    target: label.clone(),
                    title: format!("Elevated memory {mem}% on {}", guest.name),
                    desc: format!("{label} is using {mem}% of its allocated memory on node {node}."),
                    rule: format!("guest.mem.utilization >= {}%", th.guest_mem_warn),
                });
            }
            if cpu >= th.guest_cpu_warn {
                cands.push(Candidate {
                    key: format!("pmx:{}:{}:{}:cpu", data.server, node, guest.id),
                    sev: "warn".to_string(),
                    source: "proxmox".to_string(),
                    host: format!("{} / {}", data.server, node),
                    target: label.clone(),
                    title: format!("Sustained CPU {cpu}% on {}", guest.name),
                    desc: format!("{label} is consuming {cpu}% CPU on node {node}."),
                    rule: format!("guest.cpu.utilization >= {}%", th.guest_cpu_warn),
                });
            }
            if disk >= th.guest_disk_warn {
                cands.push(Candidate {
                    key: format!("pmx:{}:{}:{}:disk", data.server, node, guest.id),
                    sev: "warn".to_string(),
                    source: "proxmox".to_string(),
                    host: format!("{} / {}", data.server, node),
                    target: label,
                    title: format!("Disk {disk}% full on {}", guest.name),
                    desc: format!("{} root volume is {disk}% full on node {node}.", guest.name),
                    rule: format!("guest.disk.utilization >= {}%", th.guest_disk_warn),
                });
            }
        }
        guests_by_node.entry(node).or_default().push(guest);
    }

    // Node tiles.
    for r in &data.resources {
        if r.kind != "node" {
            continue;
        }
        let Some(node) = r.node.clone() else { continue };
        let online = r.status.as_deref() == Some("online");
        let cpu = (r.cpu * 100.0).round() as u32;
        let mem = pct(r.mem, r.maxmem);
        let disk = pct(r.disk, r.maxdisk);
        let rrd = data.node_rrd.get(&node);
        let net_mbps = rrd
            .map(|p| (p.netin.unwrap_or(0.0) + p.netout.unwrap_or(0.0)) * 8.0 / 1_000_000.0)
            .unwrap_or(0.0);
        let net = (net_mbps / 10.0).round().clamp(0.0, 100.0) as u32;
        let guests = guests_by_node.get(&node).cloned().unwrap_or_default();
        let gc = GuestCount {
            vm: guests.iter().filter(|g| g.kind == "vm").count() as u32,
            lxc: guests.iter().filter(|g| g.kind == "lxc").count() as u32,
        };
        let status = if !online {
            "crit"
        } else if mem >= th.node_mem_crit || cpu >= th.node_cpu_crit {
            "crit"
        } else if mem >= th.node_mem_warn || cpu >= th.node_cpu_warn || disk >= th.node_disk_warn {
            "warn"
        } else {
            "ok"
        };

        if online {
            if mem >= th.node_mem_crit {
                cands.push(Candidate {
                    key: format!("pmx:{}:{}:node-mem", data.server, node),
                    sev: "crit".to_string(),
                    source: "proxmox".to_string(),
                    host: format!("{} / {}", data.server, node),
                    target: format!("node {node}"),
                    title: format!("Node {node} memory at {mem}%"),
                    desc: format!("Proxmox node {node} on {} is at {mem}% memory utilization.", data.server),
                    rule: format!("node.mem.utilization >= {}%", th.node_mem_crit),
                });
            }
            if disk >= th.node_disk_warn {
                cands.push(Candidate {
                    key: format!("pmx:{}:{}:node-disk", data.server, node),
                    sev: "warn".to_string(),
                    source: "proxmox".to_string(),
                    host: format!("{} / {}", data.server, node),
                    target: format!("node {node}"),
                    title: format!("Node {node} root filesystem {disk}% full"),
                    desc: format!("Proxmox node {node} root filesystem is {disk}% full."),
                    rule: format!("node.rootfs.utilization >= {}%", th.node_disk_warn),
                });
            }
        } else {
            cands.push(Candidate {
                key: format!("pmx:{}:{}:node-offline", data.server, node),
                sev: "crit".to_string(),
                source: "proxmox".to_string(),
                host: format!("{} / {}", data.server, node),
                target: format!("node {node}"),
                title: format!("Proxmox node {node} is offline"),
                desc: format!("Node {node} on {} is not responding to the cluster.", data.server),
                rule: "node.status == offline".to_string(),
            });
        }

        tiles.push(NodeTile {
            name: node.clone(),
            server: data.server.clone(),
            host: data.server.clone(),
            status: status.to_string(),
            cpu,
            mem,
            disk,
            net,
            net_mbps: (net_mbps * 10.0).round() / 10.0,
            guests: gc,
            model: format!("PVE {} · {} cores", data.release, r.maxcpu.round() as u32),
            uptime: fmt_uptime(r.uptime),
        });
    }

    // Guest storage (volumes that back VM/CT disks).
    for r in &data.resources {
        if r.kind != "storage" {
            continue;
        }
        let content = r.content.as_deref().unwrap_or("");
        if content.contains("rootdir") || content.contains("images") {
            storage_used += r.disk;
            storage_total += r.maxdisk;
        }
    }

    // Task log → events.
    for t in &data.tasks {
        let ts_epoch = t.endtime.unwrap_or(t.starttime);
        let dt: DateTime<Local> = DateTime::from_timestamp(ts_epoch, 0)
            .unwrap_or_default()
            .with_timezone(&Local);
        let status = t.status.clone().unwrap_or_else(|| "running".to_string());
        let level = if t.status.is_none() {
            "warn"
        } else if status == "OK" {
            "info"
        } else {
            "error"
        };
        let friendly = friendly_task(&t.kind);
        let id_part = t
            .id
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| format!(" {s}"))
            .unwrap_or_default();
        let msg = if status == "OK" {
            format!("{friendly}{id_part} completed successfully")
        } else {
            format!("{friendly}{id_part} ended: {status}")
        };
        events.push(Event {
            ts: dt.to_rfc3339(),
            time: dt.format("%H:%M:%S").to_string(),
            level: level.to_string(),
            source: format!("proxmox/{}", t.node),
            source_kind: "proxmox".to_string(),
            target: t
                .id
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| t.kind.clone()),
            msg,
        });
    }
    let _ = now;

    let node_guests: Vec<NodeGuests> = tiles
        .iter()
        .map(|tile| {
            let mut guests = guests_by_node.get(&tile.name).cloned().unwrap_or_default();
            guests.sort_by_key(|g| g.id);
            NodeGuests {
                node: tile.name.clone(),
                server: tile.server.clone(),
                guests,
            }
        })
        .collect();

    (tiles, node_guests, events, cands, storage_used, storage_total)
}

// ── UniFi processing ────────────────────────────────────────────────────────

#[allow(clippy::type_complexity)]
fn process_unifi(
    data: &UnifiData,
    now: i64,
    th: &AlertThresholds,
) -> (UnifiView, TopoNode, Vec<Event>, Vec<Candidate>, f64, f64) {
    // Clients per device, by the device they connect through.
    let mut clients_per_device: HashMap<String, u32> = HashMap::new();
    let mut wireless = 0u32;
    let mut wired = 0u32;
    for c in &data.clients {
        match c.kind.as_deref() {
            Some("WIRELESS") => wireless += 1,
            Some("WIRED") => wired += 1,
            _ => {}
        }
        if let Some(id) = &c.uplink_device_id {
            *clients_per_device.entry(id.clone()).or_insert(0) += 1;
        }
    }

    let mut devices: Vec<UniDeviceOut> = Vec::new();
    let mut events: Vec<Event> = Vec::new();
    let mut cands: Vec<Candidate> = Vec::new();
    let mut uplink_of: HashMap<String, Option<String>> = HashMap::new();
    let mut poe_active = 0u32;
    let mut poe_capable = 0u32;
    let mut wan_down = 0.0;
    let mut wan_up = 0.0;

    for b in &data.devices {
        let model = b.list.model.clone().unwrap_or_default();
        let online = b.list.state.as_deref() == Some("ONLINE");
        let features = &b.list.features;
        let is_ap = features.iter().any(|f| f == "accessPoint");
        let is_gateway = model.contains("UCG")
            || model.contains("UDM")
            || model.contains("UXG")
            || model.contains("UDR");
        let kind = if is_gateway {
            "Gateway"
        } else if is_ap {
            "Access Point"
        } else if model.contains("UPS") {
            "UPS"
        } else {
            "Switch"
        };

        let cpu = b.stats.cpu_utilization_pct.unwrap_or(0.0).round() as u32;
        let mem = b.stats.memory_utilization_pct.unwrap_or(0.0).round() as u32;
        let status = if !online {
            "crit"
        } else if cpu >= th.unifi_cpu_warn || mem >= th.unifi_mem_warn {
            "warn"
        } else {
            "ok"
        };

        let tx_bps = b.stats.uplink.as_ref().and_then(|u| u.tx_rate_bps).unwrap_or(0);
        let rx_bps = b.stats.uplink.as_ref().and_then(|u| u.rx_rate_bps).unwrap_or(0);
        let tx_mbps = tx_bps as f64 * 8.0 / 1_000_000.0;
        let rx_mbps = rx_bps as f64 * 8.0 / 1_000_000.0;

        let ports: Vec<PortOut> = b
            .detail
            .interfaces
            .as_ref()
            .map(|i| {
                i.ports
                    .iter()
                    .map(|p| {
                        let up = p.state.as_deref() == Some("UP");
                        let poe_up = p.poe.as_ref().map(|x| x.state.as_deref() == Some("UP")).unwrap_or(false);
                        let poe_cap = p.poe.as_ref().map(|x| x.enabled).unwrap_or(false);
                        if poe_cap {
                            poe_capable += 1;
                        }
                        if poe_up {
                            poe_active += 1;
                        }
                        PortOut {
                            idx: p.idx,
                            up,
                            poe: poe_up,
                            speed_mbps: p.speed_mbps.unwrap_or(0),
                            connector: p.connector.clone().unwrap_or_default(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let radios: Vec<RadioOut> = b
            .detail
            .interfaces
            .as_ref()
            .map(|i| {
                i.radios
                    .iter()
                    .map(|r| {
                        let band = match r.frequency_ghz {
                            Some(f) if f < 3.0 => "2.4 GHz",
                            Some(f) if f < 5.9 => "5 GHz",
                            Some(_) => "6 GHz",
                            None => "—",
                        };
                        RadioOut {
                            band: band.to_string(),
                            channel: r.channel.unwrap_or(0),
                            width: r.channel_width_mhz.unwrap_or(0),
                            standard: r.wlan_standard.clone().unwrap_or_default(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let dev = UniDeviceOut {
            id: b.list.id.clone(),
            name: b.list.name.clone().unwrap_or_else(|| "unnamed".to_string()),
            kind: kind.to_string(),
            model,
            ip: b.list.ip_address.clone().unwrap_or_default(),
            mac: b.list.mac_address.clone().unwrap_or_default(),
            status: status.to_string(),
            uptime: fmt_uptime(b.stats.uptime_sec.unwrap_or(0)),
            clients: clients_per_device.get(&b.list.id).copied().unwrap_or(0),
            tx_mbps: (tx_mbps * 100.0).round() / 100.0,
            rx_mbps: (rx_mbps * 100.0).round() / 100.0,
            fw: b.list.firmware_version.clone().unwrap_or_default(),
            site: data.site.clone(),
            cpu,
            mem,
            firmware_updatable: b.list.firmware_updatable,
            ports,
            radios,
        };

        if is_gateway {
            wan_down = (rx_mbps * 100.0).round() / 100.0;
            wan_up = (tx_mbps * 100.0).round() / 100.0;
        }

        // Alerts from device state.
        if !online {
            cands.push(Candidate {
                key: format!("unifi:{}:offline", dev.id),
                sev: "crit".to_string(),
                source: "unifi".to_string(),
                host: format!("UniFi · {}", data.site),
                target: format!("{} · {}", dev.name, dev.ip),
                title: format!("{} '{}' is offline", dev.kind, dev.name),
                desc: format!(
                    "UniFi {} '{}' ({}) is not responding to the controller.",
                    dev.kind, dev.name, dev.model
                ),
                rule: "device.state == OFFLINE".to_string(),
            });
            events.push(Event {
                ts: Utc::now().to_rfc3339(),
                time: Local::now().format("%H:%M:%S").to_string(),
                level: "error".to_string(),
                source: format!("unifi/{}", dev.name),
                source_kind: "unifi".to_string(),
                target: "heartbeat".to_string(),
                msg: format!("device unreachable — {} '{}' offline", dev.kind, dev.name),
            });
        } else if cpu >= th.unifi_cpu_warn || mem >= th.unifi_mem_warn {
            cands.push(Candidate {
                key: format!("unifi:{}:load", dev.id),
                sev: "warn".to_string(),
                source: "unifi".to_string(),
                host: format!("UniFi · {}", data.site),
                target: format!("{} · {}", dev.name, dev.ip),
                title: format!("{} '{}' under high load", dev.kind, dev.name),
                desc: format!("'{}' is at {cpu}% CPU / {mem}% memory.", dev.name),
                rule: format!(
                    "device.cpu >= {}% or device.mem >= {}%",
                    th.unifi_cpu_warn, th.unifi_mem_warn
                ),
            });
        }
        if b.list.firmware_updatable {
            cands.push(Candidate {
                key: format!("unifi:{}:fw", dev.id),
                sev: "info".to_string(),
                source: "unifi".to_string(),
                host: format!("UniFi · {}", data.site),
                target: format!("{} · {}", dev.name, dev.ip),
                title: format!("Firmware update available for {}", dev.name),
                desc: format!(
                    "A newer firmware is available for {} '{}' (current {}).",
                    dev.model, dev.name, dev.fw
                ),
                rule: "device.firmwareUpdatable == true".to_string(),
            });
        }

        // Provisioning event (real timestamp from the controller).
        if let Some(prov) = &b.detail.provisioned_at {
            if let Ok(dt) = DateTime::parse_from_rfc3339(prov) {
                let local = dt.with_timezone(&Local);
                events.push(Event {
                    ts: dt.to_rfc3339(),
                    time: local.format("%H:%M:%S").to_string(),
                    level: "info".to_string(),
                    source: format!("unifi/{}", dev.name),
                    source_kind: "unifi".to_string(),
                    target: "provision".to_string(),
                    msg: format!("{} '{}' provisioned · fw {}", dev.kind, dev.name, dev.fw),
                });
            }
        }

        uplink_of.insert(
            dev.id.clone(),
            b.detail.uplink.as_ref().and_then(|u| u.device_id.clone()),
        );
        devices.push(dev);
    }

    devices.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));

    let topology = build_topology(&devices, &uplink_of, wan_down, wan_up);
    let _ = now;

    let view = UnifiView {
        kpis: Vec::new(),
        devices,
        poe_active,
        poe_capable,
        wireless_clients: wireless,
        wired_clients: wired,
    };
    (view, topology, events, cands, wan_down, wan_up)
}

fn topo_kind(d: &UniDeviceOut) -> &'static str {
    match d.kind.as_str() {
        "Gateway" => "router",
        "Access Point" => "ap",
        _ => {
            if d.model.contains("Aggregation") || d.model.contains("EnterpriseXG") {
                "agg"
            } else {
                "sw"
            }
        }
    }
}

fn build_topology(
    devices: &[UniDeviceOut],
    uplink_of: &HashMap<String, Option<String>>,
    wan_down: f64,
    wan_up: f64,
) -> TopoNode {
    let by_id: HashMap<&str, &UniDeviceOut> = devices.iter().map(|d| (d.id.as_str(), d)).collect();

    // children[parent_id] = [child_id, ...]
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for d in devices {
        if let Some(Some(parent)) = uplink_of.get(&d.id) {
            if by_id.contains_key(parent.as_str()) {
                children.entry(parent.clone()).or_default().push(d.id.clone());
            }
        }
    }

    // Primary root: the gateway. Fallback: first device without a valid uplink.
    let gateway = devices.iter().find(|d| d.kind == "Gateway");
    let root_dev = gateway.or_else(|| {
        devices.iter().find(|d| {
            !matches!(uplink_of.get(&d.id), Some(Some(p)) if by_id.contains_key(p.as_str()))
        })
    });
    let Some(root_dev) = root_dev else {
        return TopoNode::default();
    };

    // Devices that have no valid parent and are not the root → attach to root.
    let orphans: Vec<String> = devices
        .iter()
        .filter(|d| d.id != root_dev.id)
        .filter(|d| !matches!(uplink_of.get(&d.id), Some(Some(p)) if by_id.contains_key(p.as_str())))
        .map(|d| d.id.clone())
        .collect();
    if !orphans.is_empty() {
        children.entry(root_dev.id.clone()).or_default().extend(orphans);
    }

    let mut visited = std::collections::HashSet::new();
    build_topo_node(root_dev, &by_id, &children, &mut visited, wan_down, wan_up)
}

fn build_topo_node(
    dev: &UniDeviceOut,
    by_id: &HashMap<&str, &UniDeviceOut>,
    children: &HashMap<String, Vec<String>>,
    visited: &mut std::collections::HashSet<String>,
    wan_down: f64,
    wan_up: f64,
) -> TopoNode {
    visited.insert(dev.id.clone());
    let kind = topo_kind(dev);
    let up_ports = dev.ports.iter().filter(|p| p.up).count();
    let total_ports = dev.ports.len();

    let mut kids: Vec<TopoNode> = Vec::new();
    if let Some(ids) = children.get(&dev.id) {
        for id in ids.clone() {
            if visited.contains(&id) {
                continue;
            }
            if let Some(c) = by_id.get(id.as_str()).copied() {
                kids.push(build_topo_node(c, by_id, children, visited, wan_down, wan_up));
            }
        }
    }
    kids.sort_by(|a, b| topo_rank(&a.kind).cmp(&topo_rank(&b.kind)).then(a.name.cmp(&b.name)));

    TopoNode {
        kind: kind.to_string(),
        id: dev.id.clone(),
        name: dev.name.clone(),
        model: dev.model.clone(),
        ip: dev.ip.clone(),
        status: dev.status.clone(),
        clients: dev.clients,
        ports: if total_ports > 0 {
            format!("{up_ports}/{total_ports} ports up")
        } else {
            String::new()
        },
        wan: if kind == "router" {
            format!("↓ {} · ↑ {}", fmt_mbps(wan_down), fmt_mbps(wan_up))
        } else {
            String::new()
        },
        children: kids,
    }
}

fn topo_rank(kind: &str) -> u8 {
    match kind {
        "router" => 0,
        "agg" => 1,
        "sw" => 2,
        _ => 3,
    }
}

fn count_topology(root: &TopoNode) -> TopoCounts {
    let mut c = TopoCounts::default();
    fn walk(n: &TopoNode, c: &mut TopoCounts) {
        match n.kind.as_str() {
            "router" => c.router += 1,
            "ap" => c.ap += 1,
            _ => c.sw += 1,
        }
        match n.status.as_str() {
            "ok" => c.ok += 1,
            "warn" => c.warn += 1,
            _ => c.crit += 1,
        }
        c.total += 1;
        for k in &n.children {
            walk(k, c);
        }
    }
    if !root.id.is_empty() {
        walk(root, &mut c);
    }
    c
}

// ── Alerts / Events views ───────────────────────────────────────────────────

fn build_alerts_view(alerts: Vec<Alert>, history: &History) -> AlertsView {
    let open = alerts.iter().filter(|a| a.status == "open").count() as u32;
    let ack = alerts.iter().filter(|a| a.status == "ack").count() as u32;
    let resolved = alerts.iter().filter(|a| a.status == "resolved").count() as u32;
    let crit = alerts
        .iter()
        .filter(|a| a.sev == "crit" && a.status != "resolved")
        .count() as u32;
    let warn = alerts
        .iter()
        .filter(|a| a.sev == "warn" && a.status != "resolved")
        .count() as u32;

    // 24h histogram: bucket each alert by the hour it was first raised.
    let mut hist = vec![0u32; 24];
    for a in &alerts {
        let hours_ago = (a.age_min / 60).clamp(0, 23) as usize;
        hist[23 - hours_ago] += 1;
    }

    let kpis = vec![
        kpi(
            open.to_string(),
            "",
            format!("{crit} critical · {warn} warning"),
            history.trend(24, |s| s.active_alerts),
            history.spark(24, |s| s.active_alerts),
        ),
        kpi(
            ack.to_string(),
            "",
            "awaiting resolution".to_string(),
            0.0,
            history.spark(24, |s| s.alerts_warn),
        ),
        kpi(
            resolved.to_string(),
            "",
            "this session".to_string(),
            0.0,
            history.spark(24, |s| (s.active_alerts - s.alerts_crit).max(0.0)),
        ),
        kpi(
            (alerts.len() as u32).to_string(),
            "",
            "tracked conditions".to_string(),
            history.trend(24, |s| s.active_alerts),
            history.spark(24, |s| s.alerts_crit),
        ),
    ];

    AlertsView {
        kpis,
        alerts,
        histogram: hist,
    }
}

fn build_events_view(events: Vec<Event>, history: &History) -> EventsView {
    let errors = events.iter().filter(|e| e.level == "error").count() as u32;
    let warns = events.iter().filter(|e| e.level == "warn").count() as u32;

    // Most active source.
    let mut by_source: HashMap<&str, u32> = HashMap::new();
    for e in &events {
        *by_source.entry(e.source_kind.as_str()).or_insert(0) += 1;
    }
    let top_source = by_source
        .iter()
        .max_by_key(|(_, n)| **n)
        .map(|(k, _)| *k)
        .unwrap_or("—");
    let top_label = match top_source {
        "proxmox" => "Proxmox",
        "unifi" => "UniFi",
        other => other,
    };

    // Event-rate chart: 60 buckets across the observed event time span.
    let mut times: Vec<i64> = events
        .iter()
        .filter_map(|e| DateTime::parse_from_rfc3339(&e.ts).ok())
        .map(|d| d.timestamp())
        .collect();
    times.sort_unstable();
    let mut rate = vec![0u32; 60];
    if let (Some(&min), Some(&max)) = (times.first(), times.last()) {
        let span = (max - min).max(1);
        for t in &times {
            let idx = (((t - min) as f64 / span as f64) * 59.0).round() as usize;
            rate[idx.min(59)] += 1;
        }
    }

    let kpis = vec![
        kpi(
            (events.len() as u32).to_string(),
            "",
            format!("{} sources", by_source.len()),
            0.0,
            history.spark(24, |s| s.events_total),
        ),
        kpi(
            errors.to_string(),
            "",
            format!("{warns} warnings"),
            history.trend(24, |s| s.error_events),
            history.spark(24, |s| s.error_events),
        ),
        kpi(
            events
                .iter()
                .filter(|e| e.target == "vzdump" || e.msg.starts_with("Backup"))
                .count()
                .to_string(),
            "",
            "backup tasks".to_string(),
            0.0,
            history.spark(24, |s| s.events_total),
        ),
        kpi(
            top_label.to_string(),
            "",
            format!(
                "{}% of events",
                pct(
                    by_source.get(top_source).copied().unwrap_or(0) as u64,
                    events.len().max(1) as u64
                )
            ),
            0.0,
            history.spark(24, |s| s.events_total),
        ),
    ];

    EventsView { kpis, events, rate }
}

// ── Bandwidth series ────────────────────────────────────────────────────────

fn build_bandwidth(history: &History) -> BandwidthSeries {
    let samples = history.window(86_400, 120);
    let down: Vec<f64> = samples.iter().map(|s| s.wan_down_mbps).collect();
    let up: Vec<f64> = samples.iter().map(|s| s.wan_up_mbps).collect();
    let points = samples.len();

    let peak_down = down.iter().cloned().fold(0.0, f64::max);
    let peak_up = up.iter().cloned().fold(0.0, f64::max);
    let totals: Vec<f64> = samples
        .iter()
        .map(|s| s.wan_down_mbps + s.wan_up_mbps)
        .collect();
    let avg = if totals.is_empty() {
        0.0
    } else {
        totals.iter().sum::<f64>() / totals.len() as f64
    };

    // Approximate transferred volume by integrating throughput over time.
    let mut transferred_gb = 0.0;
    for w in samples.windows(2) {
        let dt = (w[1].t - w[0].t).max(0) as f64;
        let avg_mbps = (w[0].wan_down_mbps
            + w[0].wan_up_mbps
            + w[1].wan_down_mbps
            + w[1].wan_up_mbps)
            / 2.0;
        transferred_gb += avg_mbps * dt / 8.0 / 1000.0;
    }

    let window_label = if points < 2 {
        "collecting WAN history".to_string()
    } else {
        let span = samples.last().unwrap().t - samples.first().unwrap().t;
        if span >= 23 * 3600 {
            "last 24h".to_string()
        } else if span >= 3600 {
            format!("last {}h {}m", span / 3600, (span % 3600) / 60)
        } else {
            format!("last {}m", (span / 60).max(1))
        }
    };

    BandwidthSeries {
        down,
        up,
        points,
        window_label,
        peak_down: (peak_down * 10.0).round() / 10.0,
        peak_up: (peak_up * 10.0).round() / 10.0,
        avg: (avg * 10.0).round() / 10.0,
        transferred_gb: (transferred_gb * 100.0).round() / 100.0,
    }
}
