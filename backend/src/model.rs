//! Output model — the JSON contract served to the frontend.
//!
//! Every numeric KPI is pre-formatted server-side into a [`Kpi`] so the
//! frontend only renders strings; pages still receive structured lists
//! (nodes, guests, devices, alerts, events) for tables and detail panels.

use serde::Serialize;

/// One pre-formatted KPI tile.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Kpi {
    /// Primary value, ready to print (e.g. `"99.98"`, `"1.84"`, `"45"`).
    pub display: String,
    /// Unit suffix shown next to the value (e.g. `"%"`, `"Gbps"`, `"/47"`).
    pub unit: String,
    /// Secondary descriptive line.
    pub sub: String,
    /// Signed trend; the frontend hides the indicator when this is `0`.
    pub trend: f64,
    /// Recent values for the inline sparkline.
    pub spark: Vec<f64>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SourceHealth {
    pub name: String,
    pub kind: String,
    pub ok: bool,
    pub stale: bool,
    pub failure_count: u32,
    pub retry_in_sec: Option<u64>,
    pub last_ok_ago_sec: Option<u64>,
    pub detail: String,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct GuestCount {
    pub vm: u32,
    pub lxc: u32,
}

/// A Proxmox node tile (dashboard + Proxmox page section headers).
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct NodeTile {
    pub name: String,
    pub server: String,
    pub host: String,
    pub status: String,
    pub cpu: u32,
    pub mem: u32,
    pub disk: u32,
    pub net: u32,
    pub net_mbps: f64,
    pub guests: GuestCount,
    pub model: String,
    pub uptime: String,
}

/// A Proxmox guest (VM or LXC container).
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Guest {
    pub id: u64,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub cpu: u32,
    pub mem: u32,
    pub disk: u32,
    pub net: u32,
    pub uptime: String,
    pub tags: String,
    pub cores: u32,
    pub ram: String,
    pub node: String,
    pub server: String,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct NodeGuests {
    pub node: String,
    pub server: String,
    pub guests: Vec<Guest>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub sev: String,
    pub title: String,
    pub source: String,
    pub time: String,
}

/// 24h bandwidth time-series for the dashboard chart.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct BandwidthSeries {
    pub down: Vec<f64>,
    pub up: Vec<f64>,
    pub points: usize,
    pub window_label: String,
    pub peak_down: f64,
    pub peak_up: f64,
    pub avg: f64,
    pub transferred_gb: f64,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TopoCounts {
    pub router: u32,
    pub sw: u32,
    pub ap: u32,
    pub ok: u32,
    pub warn: u32,
    pub crit: u32,
    pub total: u32,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TopoNode {
    pub kind: String,
    pub id: String,
    pub name: String,
    pub model: String,
    pub ip: String,
    pub status: String,
    pub clients: u32,
    pub ports: String,
    pub wan: String,
    pub children: Vec<TopoNode>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub kpis: Vec<Kpi>,
    pub issues: Vec<Issue>,
    pub bandwidth: BandwidthSeries,
    pub nodes: Vec<NodeTile>,
    pub topology_counts: TopoCounts,
    pub total_guests: u32,
    pub quorum: Option<String>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxmoxView {
    pub kpis: Vec<Kpi>,
    pub nodes: Vec<NodeTile>,
    pub guests: Vec<NodeGuests>,
    pub high_cpu: u32,
    pub high_mem: u32,
    pub running: u32,
    pub stopped: u32,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PortOut {
    pub idx: u32,
    pub up: bool,
    pub poe: bool,
    pub speed_mbps: u32,
    pub connector: String,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RadioOut {
    pub band: String,
    pub channel: u32,
    pub width: u32,
    pub standard: String,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UniDeviceOut {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub model: String,
    pub ip: String,
    pub mac: String,
    pub status: String,
    pub uptime: String,
    pub clients: u32,
    pub tx_mbps: f64,
    pub rx_mbps: f64,
    pub fw: String,
    pub site: String,
    pub cpu: u32,
    pub mem: u32,
    pub firmware_updatable: bool,
    pub ports: Vec<PortOut>,
    pub radios: Vec<RadioOut>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UnifiView {
    pub kpis: Vec<Kpi>,
    pub devices: Vec<UniDeviceOut>,
    pub poe_active: u32,
    pub poe_capable: u32,
    pub wireless_clients: u32,
    pub wired_clients: u32,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UnraidStorageOut {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub status: String,
    pub used: String,
    pub total: String,
    pub used_pct: u32,
    pub members: u32,
    pub temp: String,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UnraidDiskOut {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub device: String,
    pub status: String,
    pub temp: String,
    pub size: String,
    pub used: String,
    pub used_pct: u32,
    pub spinning: Option<bool>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UnraidContainerOut {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub cpu: u32,
    pub mem: u32,
    pub memory: String,
    pub net_io: String,
    pub block_io: String,
    pub auto_start: bool,
    pub update_available: bool,
    pub update_status: String,
    pub root_fs: String,
    pub writable: String,
    pub log_size: String,
    pub network: String,
    pub ports: String,
    pub orphaned: bool,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UnraidVmOut {
    pub id: String,
    pub name: String,
    pub state: String,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UnraidNotificationOut {
    pub title: String,
    pub importance: String,
    pub time: String,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UnraidServerOut {
    pub name: String,
    pub source: String,
    pub host: String,
    pub status: String,
    pub lan_ip: String,
    pub local_url: String,
    pub version: String,
    pub api_version: String,
    pub kernel: String,
    pub uptime: String,
    pub cpu_brand: String,
    pub cpu_cores: u32,
    pub cpu_threads: u32,
    pub cpu: u32,
    pub mem: u32,
    pub memory: String,
    pub temp: String,
    pub temp_sensor: String,
    pub array_state: String,
    pub array_used: String,
    pub array_total: String,
    pub array_used_pct: u32,
    pub storage_used: String,
    pub storage_total: String,
    pub storage_used_pct: u32,
    pub disk_count: u32,
    pub parity_status: String,
    pub parity_progress: u32,
    pub parity_errors: u32,
    pub containers_running: u32,
    pub containers_total: u32,
    pub vms_running: u32,
    pub vms_total: u32,
    pub notification_count: u32,
    pub software_update_count: u32,
    pub storage: Vec<UnraidStorageOut>,
    pub disks: Vec<UnraidDiskOut>,
    pub containers: Vec<UnraidContainerOut>,
    pub vms: Vec<UnraidVmOut>,
    pub notifications: Vec<UnraidNotificationOut>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UnraidView {
    pub kpis: Vec<Kpi>,
    pub servers: Vec<UnraidServerOut>,
    pub containers_running: u32,
    pub containers_total: u32,
    pub vms_running: u32,
    pub vms_total: u32,
    pub array_warn: u32,
    pub software_update_count: u32,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    pub sev: String,
    pub status: String,
    pub title: String,
    pub desc: String,
    pub source: String,
    pub host: String,
    pub target: String,
    pub age_min: i64,
    pub occurrences: u32,
    pub assignee: Option<String>,
    pub rule: String,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AlertsView {
    pub kpis: Vec<Kpi>,
    pub alerts: Vec<Alert>,
    pub histogram: Vec<u32>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub ts: String,
    pub time: String,
    pub level: String,
    pub source: String,
    pub source_kind: String,
    pub target: String,
    pub msg: String,
    #[serde(skip_serializing)]
    pub dedupe_key: Option<String>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct EventsView {
    pub kpis: Vec<Kpi>,
    pub events: Vec<Event>,
    pub rate: Vec<u32>,
}

/// The complete snapshot served at `/api/snapshot`.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub generated_at: String,
    pub poll_interval_sec: u64,
    pub sources: Vec<SourceHealth>,
    pub dashboard: Dashboard,
    pub proxmox: ProxmoxView,
    pub unifi: UnifiView,
    pub unraid: UnraidView,
    pub topology: TopoNode,
    pub alerts: AlertsView,
    pub events: EventsView,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde::Serialize;
    use serde_json::Value;

    use super::*;

    fn keys<T: Serialize>(value: &T) -> BTreeSet<String> {
        let Value::Object(map) = serde_json::to_value(value).expect("serializes") else {
            panic!("expected JSON object");
        };
        map.keys().cloned().collect()
    }

    fn assert_keys<T: Serialize>(value: &T, expected: &[&str]) {
        let got = keys(value);
        let expected: BTreeSet<String> = expected.iter().map(|k| k.to_string()).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn snapshot_contract_uses_frontend_field_names() {
        assert_keys(
            &Snapshot::default(),
            &[
                "generatedAt",
                "pollIntervalSec",
                "sources",
                "dashboard",
                "proxmox",
                "unifi",
                "unraid",
                "topology",
                "alerts",
                "events",
            ],
        );
        assert!(!keys(&Snapshot::default()).contains("generated_at"));
    }

    #[test]
    fn nested_contract_uses_camel_case_for_renamed_fields() {
        assert_keys(
            &NodeTile::default(),
            &[
                "name", "server", "host", "status", "cpu", "mem", "disk", "net", "netMbps",
                "guests", "model", "uptime",
            ],
        );
        assert_keys(
            &SourceHealth::default(),
            &[
                "name",
                "kind",
                "ok",
                "stale",
                "failureCount",
                "retryInSec",
                "lastOkAgoSec",
                "detail",
                "error",
            ],
        );
        assert_keys(
            &UniDeviceOut::default(),
            &[
                "id",
                "name",
                "kind",
                "model",
                "ip",
                "mac",
                "status",
                "uptime",
                "clients",
                "txMbps",
                "rxMbps",
                "fw",
                "site",
                "cpu",
                "mem",
                "firmwareUpdatable",
                "ports",
                "radios",
            ],
        );
        assert_keys(
            &Alert::default(),
            &[
                "id",
                "sev",
                "status",
                "title",
                "desc",
                "source",
                "host",
                "target",
                "ageMin",
                "occurrences",
                "assignee",
                "rule",
            ],
        );
        assert_keys(
            &UnraidServerOut::default(),
            &[
                "name",
                "source",
                "host",
                "status",
                "lanIp",
                "localUrl",
                "version",
                "apiVersion",
                "kernel",
                "uptime",
                "cpuBrand",
                "cpuCores",
                "cpuThreads",
                "cpu",
                "mem",
                "memory",
                "temp",
                "tempSensor",
                "arrayState",
                "arrayUsed",
                "arrayTotal",
                "arrayUsedPct",
                "storageUsed",
                "storageTotal",
                "storageUsedPct",
                "diskCount",
                "parityStatus",
                "parityProgress",
                "parityErrors",
                "containersRunning",
                "containersTotal",
                "vmsRunning",
                "vmsTotal",
                "notificationCount",
                "softwareUpdateCount",
                "storage",
                "disks",
                "containers",
                "vms",
                "notifications",
            ],
        );
    }
}
