//! Network scanner settings and Nmap-backed worker.
//!
//! The HTTP app only queues scans. A separate `network-scanner-worker` process
//! consumes the queue and runs with the Docker network privileges needed for
//! fast LAN discovery.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::process::Command;

use crate::db;
use crate::engine::AppState;

pub const SETTINGS_KEY: &str = "network_scanner";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkScannerSettings {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_ranges")]
    pub ranges: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub discovery: NetworkDiscoverySettings,
    #[serde(default)]
    pub port_scan: NetworkPortScanSettings,
    #[serde(default)]
    pub schedule: NetworkScanSchedule,
    #[serde(default = "default_retention_days")]
    pub retention_days: i64,
}

impl Default for NetworkScannerSettings {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            ranges: default_ranges(),
            exclude: Vec::new(),
            discovery: NetworkDiscoverySettings::default(),
            port_scan: NetworkPortScanSettings::default(),
            schedule: NetworkScanSchedule::default(),
            retention_days: default_retention_days(),
        }
    }
}

impl NetworkScannerSettings {
    pub fn normalized(mut self) -> anyhow::Result<Self> {
        self.ranges = normalize_targets(self.ranges, "range")?;
        self.exclude = normalize_targets(self.exclude, "exclude target")?;
        if self.ranges.is_empty() {
            bail!("at least one scan range is required");
        }

        self.discovery.max_retries = self.discovery.max_retries.min(10);
        self.discovery.dns_servers = normalize_targets(self.discovery.dns_servers, "DNS server")?;
        self.discovery.host_timeout_ms = self.discovery.host_timeout_ms.clamp(250, 60_000);
        self.discovery.overall_timeout_sec = self.discovery.overall_timeout_sec.clamp(10, 3_600);
        self.discovery.timing_template = self.discovery.timing_template.min(5);
        self.discovery.min_rate = self.discovery.min_rate.min(1_000_000);

        if self.port_scan.ports.trim().is_empty() {
            self.port_scan.ports = NetworkPortScanSettings::default().ports;
        }
        self.port_scan.ports = self.port_scan.ports.trim().to_string();
        self.schedule.interval_minutes = self.schedule.interval_minutes.clamp(5, 10_080);
        self.retention_days = self.retention_days.clamp(1, 3_650);
        Ok(self)
    }
}

fn default_enabled() -> bool {
    true
}

fn default_ranges() -> Vec<String> {
    vec!["10.10.0.0/23".to_string()]
}

fn default_retention_days() -> i64 {
    90
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDiscoverySettings {
    #[serde(default)]
    pub method: DiscoveryMethod,
    #[serde(default)]
    pub dns_resolution: bool,
    #[serde(default)]
    pub dns_servers: Vec<String>,
    #[serde(default = "default_max_retries")]
    pub max_retries: u8,
    #[serde(default = "default_host_timeout_ms")]
    pub host_timeout_ms: u64,
    #[serde(default = "default_overall_timeout_sec")]
    pub overall_timeout_sec: u64,
    #[serde(default = "default_timing_template")]
    pub timing_template: u8,
    #[serde(default = "default_min_rate")]
    pub min_rate: u32,
}

impl Default for NetworkDiscoverySettings {
    fn default() -> Self {
        Self {
            method: DiscoveryMethod::Auto,
            dns_resolution: false,
            dns_servers: Vec::new(),
            max_retries: default_max_retries(),
            host_timeout_ms: default_host_timeout_ms(),
            overall_timeout_sec: default_overall_timeout_sec(),
            timing_template: default_timing_template(),
            min_rate: default_min_rate(),
        }
    }
}

fn default_max_retries() -> u8 {
    1
}

fn default_host_timeout_ms() -> u64 {
    2_500
}

fn default_overall_timeout_sec() -> u64 {
    120
}

fn default_timing_template() -> u8 {
    4
}

fn default_min_rate() -> u32 {
    5_000
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryMethod {
    #[default]
    Auto,
    Arp,
    IcmpTcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkPortScanSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub profile: PortProfile,
    #[serde(default = "default_ports")]
    pub ports: String,
    #[serde(default)]
    pub service_detection: bool,
    #[serde(default)]
    pub os_detection: bool,
    #[serde(default)]
    pub scan_technique: PortScanTechnique,
    #[serde(default)]
    pub udp_scan: bool,
    #[serde(default = "default_only_scan_discovered")]
    pub only_scan_discovered: bool,
    #[serde(default = "default_skip_host_discovery")]
    pub skip_host_discovery: bool,
}

impl Default for NetworkPortScanSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            profile: PortProfile::Fast,
            ports: default_ports(),
            service_detection: false,
            os_detection: false,
            scan_technique: PortScanTechnique::Syn,
            udp_scan: false,
            only_scan_discovered: true,
            skip_host_discovery: true,
        }
    }
}

fn default_ports() -> String {
    "22,53,80,443,445,3389,8006,8080,8443,9200".to_string()
}

fn default_only_scan_discovered() -> bool {
    true
}

fn default_skip_host_discovery() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PortProfile {
    #[default]
    Fast,
    Top100,
    Top1000,
    Custom,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PortScanTechnique {
    #[default]
    Syn,
    Connect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkScanSchedule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_schedule_interval")]
    pub interval_minutes: u64,
    #[serde(default)]
    pub run_at_start: bool,
}

impl Default for NetworkScanSchedule {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_minutes: default_schedule_interval(),
            run_at_start: false,
        }
    }
}

fn default_schedule_interval() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkScanPort {
    pub protocol: String,
    pub port: u16,
    pub state: String,
    pub service: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkScanDevice {
    pub ip: String,
    pub hostname: Option<String>,
    pub mac: Option<String>,
    pub vendor: Option<String>,
    pub status: String,
    pub discovery_method: String,
    pub latency_ms: Option<f64>,
    pub ports: Vec<NetworkScanPort>,
    pub os_guess: Option<String>,
    pub last_seen: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkScanSummary {
    pub scanner: String,
    pub range_count: usize,
    pub hosts_up: usize,
    pub open_ports: usize,
    pub duration_ms: u128,
    pub discovery_method: DiscoveryMethod,
    pub port_scan_enabled: bool,
}

#[derive(Debug, Clone)]
struct ScanOutcome {
    devices: Vec<NetworkScanDevice>,
    summary: NetworkScanSummary,
}

pub async fn run_worker(pool: PgPool) -> anyhow::Result<()> {
    match nmap_version().await {
        Ok(v) => tracing::info!("network scanner worker using {v}"),
        Err(e) => tracing::warn!("network scanner worker cannot verify nmap: {e:#}"),
    }

    loop {
        match db::try_claim_network_scan_job(&pool).await {
            Ok(Some(job)) => {
                if let Err(e) = run_job(&pool, job).await {
                    tracing::warn!("network scan job failed: {e:#}");
                }
            }
            Ok(None) => tokio::time::sleep(Duration::from_secs(2)).await,
            Err(e) => {
                tracing::warn!("could not claim network scan job: {e:#}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn run_job(pool: &PgPool, job: db::NetworkScanJobRow) -> anyhow::Result<()> {
    tracing::info!("network scan job {} started", job.id);
    let settings = match serde_json::from_value::<NetworkScannerSettings>(job.settings.clone())
        .context("decoding network scanner job settings")
        .and_then(NetworkScannerSettings::normalized)
    {
        Ok(s) => s,
        Err(e) => {
            db::fail_network_scan_job(pool, job.id, &format!("{e:#}")).await?;
            return Err(e);
        }
    };

    match run_scan(&settings).await {
        Ok(outcome) => {
            let devices: Vec<db::NetworkScanDevicePersist> = outcome
                .devices
                .iter()
                .map(device_to_persist)
                .collect::<anyhow::Result<_>>()?;
            db::insert_network_scan_devices(pool, job.id, &devices).await?;
            let summary = serde_json::to_value(&outcome.summary)?;
            db::complete_network_scan_job(pool, job.id, &summary).await?;
            if let Err(e) = db::prune_network_scan_history(pool, settings.retention_days).await {
                tracing::warn!("could not prune network scan history: {e:#}");
            }
            tracing::info!(
                "network scan job {} complete — {} host(s), {} open port(s)",
                job.id,
                outcome.summary.hosts_up,
                outcome.summary.open_ports
            );
        }
        Err(e) => {
            db::fail_network_scan_job(pool, job.id, &format!("{e:#}")).await?;
            return Err(e);
        }
    }
    Ok(())
}

pub async fn run_scheduler(state: Arc<AppState>) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let cfg = state.config();
        let scanner = match cfg.network_scanner.clone().normalized() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("network scanner schedule skipped: {e:#}");
                continue;
            }
        };
        if !scanner.enabled || !scanner.schedule.enabled {
            continue;
        }
        let due = match db::network_scan_schedule_due(
            &state.pool,
            scanner.schedule.interval_minutes,
            scanner.schedule.run_at_start,
        )
        .await
        {
            Ok(due) => due,
            Err(e) => {
                tracing::warn!("could not check network scan schedule: {e:#}");
                continue;
            }
        };
        if !due {
            continue;
        }
        let value = match serde_json::to_value(&scanner) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("could not encode network scan settings: {e:#}");
                continue;
            }
        };
        match db::enqueue_network_scan_job(&state.pool, "schedule", &value).await {
            Ok(id) => tracing::info!("queued scheduled network scan job {id}"),
            Err(e) => tracing::warn!("could not queue scheduled network scan: {e:#}"),
        }
    }
}

async fn run_scan(settings: &NetworkScannerSettings) -> anyhow::Result<ScanOutcome> {
    let settings = settings.clone().normalized()?;
    let started = Instant::now();

    let discovery_args = build_discovery_args(&settings);
    tracing::debug!("running nmap discovery: {:?}", discovery_args);
    let xml = run_nmap(&discovery_args, settings.discovery.overall_timeout_sec)
        .await
        .context("running nmap discovery scan")?;
    let mut devices = parse_nmap_xml(&xml, "discovery")?;

    if settings.port_scan.enabled {
        let targets = if settings.port_scan.only_scan_discovered {
            devices.iter().map(|d| d.ip.clone()).collect::<Vec<_>>()
        } else {
            settings.ranges.clone()
        };
        if !targets.is_empty() {
            let port_args = build_port_scan_args(&settings, &targets);
            tracing::debug!("running nmap port scan: {:?}", port_args);
            let xml = run_nmap(&port_args, settings.discovery.overall_timeout_sec)
                .await
                .context("running nmap port scan")?;
            let port_devices = parse_nmap_xml(&xml, "port-scan")?;
            merge_port_results(&mut devices, port_devices);
        }
    }

    sort_devices(&mut devices);
    let open_ports = devices.iter().map(|d| d.ports.len()).sum();
    let summary = NetworkScanSummary {
        scanner: "nmap".to_string(),
        range_count: settings.ranges.len(),
        hosts_up: devices.len(),
        open_ports,
        duration_ms: started.elapsed().as_millis(),
        discovery_method: settings.discovery.method,
        port_scan_enabled: settings.port_scan.enabled,
    };
    Ok(ScanOutcome { devices, summary })
}

pub fn build_discovery_args(settings: &NetworkScannerSettings) -> Vec<String> {
    let mut args = common_args(settings);
    args.push("-sn".to_string());
    match settings.discovery.method {
        DiscoveryMethod::Auto => {
            args.push("-PR".to_string());
            args.push("-PE".to_string());
            args.push("-PS22,80,443".to_string());
        }
        DiscoveryMethod::Arp => args.push("-PR".to_string()),
        DiscoveryMethod::IcmpTcp => {
            args.push("-PE".to_string());
            args.push("-PS22,80,443".to_string());
        }
    }
    args.extend(settings.ranges.iter().cloned());
    args
}

pub fn build_port_scan_args(settings: &NetworkScannerSettings, targets: &[String]) -> Vec<String> {
    let mut args = common_args(settings);
    match settings.port_scan.scan_technique {
        PortScanTechnique::Syn => args.push("-sS".to_string()),
        PortScanTechnique::Connect => args.push("-sT".to_string()),
    }
    if settings.port_scan.udp_scan {
        args.push("-sU".to_string());
    }
    if settings.port_scan.skip_host_discovery || settings.port_scan.only_scan_discovered {
        args.push("-Pn".to_string());
    }
    args.push("--open".to_string());
    match settings.port_scan.profile {
        PortProfile::Top100 => {
            args.push("--top-ports".to_string());
            args.push("100".to_string());
        }
        PortProfile::Top1000 => {
            args.push("--top-ports".to_string());
            args.push("1000".to_string());
        }
        PortProfile::Fast | PortProfile::Custom => {
            args.push("-p".to_string());
            args.push(settings.port_scan.ports.clone());
        }
    }
    if settings.port_scan.service_detection {
        args.push("-sV".to_string());
        args.push("--version-light".to_string());
    }
    if settings.port_scan.os_detection {
        args.push("-O".to_string());
        args.push("--osscan-limit".to_string());
    }
    args.extend(targets.iter().cloned());
    args
}

fn common_args(settings: &NetworkScannerSettings) -> Vec<String> {
    let mut args = vec![
        "-oX".to_string(),
        "-".to_string(),
        format!("-T{}", settings.discovery.timing_template),
        "--max-retries".to_string(),
        settings.discovery.max_retries.to_string(),
        "--host-timeout".to_string(),
        format!("{}ms", settings.discovery.host_timeout_ms),
    ];
    if !settings.discovery.dns_resolution {
        args.push("-n".to_string());
    } else if !settings.discovery.dns_servers.is_empty() {
        args.push("--dns-servers".to_string());
        args.push(settings.discovery.dns_servers.join(","));
    }
    if settings.discovery.min_rate > 0 {
        args.push("--min-rate".to_string());
        args.push(settings.discovery.min_rate.to_string());
    }
    if !settings.exclude.is_empty() {
        args.push("--exclude".to_string());
        args.push(settings.exclude.join(","));
    }
    args
}

async fn run_nmap(args: &[String], timeout_sec: u64) -> anyhow::Result<String> {
    let mut cmd = Command::new("nmap");
    cmd.args(args);
    cmd.kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(timeout_sec), cmd.output())
        .await
        .with_context(|| format!("nmap timed out after {timeout_sec}s"))?
        .context("spawning nmap")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("nmap exited with {}: {}", output.status, stderr.trim());
    }
    String::from_utf8(output.stdout).context("nmap returned non-UTF8 XML")
}

async fn nmap_version() -> anyhow::Result<String> {
    let output = Command::new("nmap")
        .arg("--version")
        .output()
        .await
        .context("running nmap --version")?;
    if !output.status.success() {
        bail!("nmap --version exited with {}", output.status);
    }
    let text = String::from_utf8(output.stdout).context("decoding nmap --version")?;
    Ok(text.lines().next().unwrap_or("nmap").to_string())
}

pub fn parse_nmap_xml(xml: &str, discovery_method: &str) -> anyhow::Result<Vec<NetworkScanDevice>> {
    let doc = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .context("parsing nmap XML")?;
    let now = Utc::now();
    let mut devices = Vec::new();
    for host in doc.descendants().filter(|n| n.has_tag_name("host")) {
        let status = host
            .children()
            .find(|n| n.has_tag_name("status"))
            .and_then(|n| n.attribute("state"))
            .unwrap_or("unknown");
        if status != "up" {
            continue;
        }
        let ip = host
            .children()
            .find(|n| n.has_tag_name("address") && n.attribute("addrtype") == Some("ipv4"))
            .and_then(|n| n.attribute("addr"))
            .map(str::to_string);
        let Some(ip) = ip else { continue };
        let mac_node = host
            .children()
            .find(|n| n.has_tag_name("address") && n.attribute("addrtype") == Some("mac"));
        let hostname = host
            .descendants()
            .find(|n| n.has_tag_name("hostname"))
            .and_then(|n| n.attribute("name"))
            .map(str::to_string);
        let latency_ms = host
            .children()
            .find(|n| n.has_tag_name("times"))
            .and_then(|n| n.attribute("srtt"))
            .and_then(|v| v.parse::<f64>().ok())
            .map(|micro| micro / 1000.0);
        let os_guess = host
            .descendants()
            .find(|n| n.has_tag_name("osmatch"))
            .and_then(|n| n.attribute("name"))
            .map(str::to_string);
        let ports = parse_ports(host);
        devices.push(NetworkScanDevice {
            ip,
            hostname,
            mac: mac_node
                .and_then(|n| n.attribute("addr"))
                .map(str::to_string),
            vendor: mac_node
                .and_then(|n| n.attribute("vendor"))
                .map(str::to_string),
            status: status.to_string(),
            discovery_method: discovery_method.to_string(),
            latency_ms,
            ports,
            os_guess,
            last_seen: now,
        });
    }
    sort_devices(&mut devices);
    Ok(devices)
}

fn parse_ports(host: roxmltree::Node<'_, '_>) -> Vec<NetworkScanPort> {
    host.descendants()
        .filter(|n| n.has_tag_name("port"))
        .filter_map(|port| {
            let protocol = port.attribute("protocol").unwrap_or("tcp").to_string();
            let port_id = port.attribute("portid")?.parse::<u16>().ok()?;
            let state = port
                .children()
                .find(|n| n.has_tag_name("state"))
                .and_then(|n| n.attribute("state"))
                .unwrap_or("unknown")
                .to_string();
            if state != "open" {
                return None;
            }
            let svc = port.children().find(|n| n.has_tag_name("service"));
            Some(NetworkScanPort {
                protocol,
                port: port_id,
                state,
                service: svc.and_then(|n| n.attribute("name")).map(str::to_string),
                product: svc.and_then(|n| n.attribute("product")).map(str::to_string),
                version: svc.and_then(|n| n.attribute("version")).map(str::to_string),
            })
        })
        .collect()
}

fn merge_port_results(base: &mut Vec<NetworkScanDevice>, port_devices: Vec<NetworkScanDevice>) {
    let mut by_ip: BTreeMap<String, usize> = base
        .iter()
        .enumerate()
        .map(|(idx, d)| (d.ip.clone(), idx))
        .collect();

    for mut incoming in port_devices {
        match by_ip.get(&incoming.ip).copied() {
            Some(idx) => {
                let existing = &mut base[idx];
                if existing.hostname.is_none() {
                    existing.hostname = incoming.hostname.take();
                }
                if existing.mac.is_none() {
                    existing.mac = incoming.mac.take();
                }
                if existing.vendor.is_none() {
                    existing.vendor = incoming.vendor.take();
                }
                if existing.os_guess.is_none() {
                    existing.os_guess = incoming.os_guess.take();
                }
                if incoming.latency_ms.is_some() {
                    existing.latency_ms = incoming.latency_ms;
                }
                existing.ports = incoming.ports;
                existing.last_seen = incoming.last_seen;
            }
            None => {
                incoming.discovery_method = "port-scan".to_string();
                by_ip.insert(incoming.ip.clone(), base.len());
                base.push(incoming);
            }
        }
    }
}

fn device_to_persist(d: &NetworkScanDevice) -> anyhow::Result<db::NetworkScanDevicePersist> {
    Ok(db::NetworkScanDevicePersist {
        ip: d.ip.clone(),
        hostname: d.hostname.clone(),
        mac: d.mac.clone(),
        vendor: d.vendor.clone(),
        status: d.status.clone(),
        discovery_method: d.discovery_method.clone(),
        latency_ms: d.latency_ms,
        ports: serde_json::to_value(&d.ports)?,
        os_guess: d.os_guess.clone(),
        raw: json!({}),
    })
}

fn normalize_targets(targets: Vec<String>, label: &str) -> anyhow::Result<Vec<String>> {
    let mut out = Vec::new();
    for raw in targets {
        for part in raw.split([',', '\n']) {
            let target = part.trim();
            if target.is_empty() {
                continue;
            }
            if !is_safe_nmap_target(target) {
                bail!("invalid {label} '{target}'");
            }
            if !out.iter().any(|v| v == target) {
                out.push(target.to_string());
            }
        }
    }
    Ok(out)
}

fn is_safe_nmap_target(target: &str) -> bool {
    if target.starts_with('-') || target.len() > 128 {
        return false;
    }
    target
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '/' | '-' | '_'))
}

fn sort_devices(devices: &mut [NetworkScanDevice]) {
    devices.sort_by(
        |a, b| match (a.ip.parse::<IpAddr>(), b.ip.parse::<IpAddr>()) {
            (Ok(ia), Ok(ib)) => ip_sort_key(ia).cmp(&ip_sort_key(ib)),
            _ => a.ip.cmp(&b.ip),
        },
    );
}

fn ip_sort_key(ip: IpAddr) -> (u8, [u8; 16]) {
    match ip {
        IpAddr::V4(v4) => {
            let mut bytes = [0u8; 16];
            bytes[12..].copy_from_slice(&v4.octets());
            (4, bytes)
        }
        IpAddr::V6(v6) => (6, v6.octets()),
    }
}

pub fn settings_to_value(settings: &NetworkScannerSettings) -> anyhow::Result<Value> {
    serde_json::to_value(settings.clone().normalized()?)
        .context("encoding network scanner settings")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_fast_discovery_args_for_requested_subnet() {
        let settings = NetworkScannerSettings {
            ranges: vec!["10.10.0.0/23".to_string()],
            exclude: vec!["10.10.0.1".to_string()],
            discovery: NetworkDiscoverySettings {
                dns_resolution: true,
                dns_servers: vec!["10.10.0.1".to_string()],
                ..NetworkDiscoverySettings::default()
            },
            ..NetworkScannerSettings::default()
        }
        .normalized()
        .unwrap();

        let args = build_discovery_args(&settings);
        assert!(args.contains(&"-sn".to_string()));
        assert!(args.contains(&"-PR".to_string()));
        assert!(args.contains(&"--exclude".to_string()));
        assert!(args.contains(&"--dns-servers".to_string()));
        assert!(args.contains(&"10.10.0.1".to_string()));
        assert!(args.contains(&"10.10.0.0/23".to_string()));
    }

    #[test]
    fn rejects_option_like_targets() {
        let settings = NetworkScannerSettings {
            ranges: vec!["--script=vuln".to_string()],
            ..NetworkScannerSettings::default()
        };
        assert!(settings.normalized().is_err());
    }

    #[test]
    fn parses_nmap_xml_hosts_and_open_ports() {
        let xml = r#"<?xml version="1.0"?>
<!DOCTYPE nmaprun>
<nmaprun>
  <host>
    <status state="up" reason="arp-response"/>
    <address addr="10.10.0.20" addrtype="ipv4"/>
    <address addr="00:11:22:33:44:55" addrtype="mac" vendor="Acme"/>
    <hostnames><hostname name="nas.local" type="PTR"/></hostnames>
    <times srtt="1200"/>
    <ports>
      <port protocol="tcp" portid="22">
        <state state="open"/>
        <service name="ssh" product="OpenSSH" version="9"/>
      </port>
      <port protocol="tcp" portid="23"><state state="closed"/></port>
    </ports>
  </host>
</nmaprun>"#;
        let devices = parse_nmap_xml(xml, "test").unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].ip, "10.10.0.20");
        assert_eq!(devices[0].hostname.as_deref(), Some("nas.local"));
        assert_eq!(devices[0].vendor.as_deref(), Some("Acme"));
        assert_eq!(devices[0].ports.len(), 1);
        assert_eq!(devices[0].ports[0].service.as_deref(), Some("ssh"));
    }
}
