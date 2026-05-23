//! Unraid GraphQL API client.
//!
//! The local Unraid API is exposed at `/graphql` and accepts API-key
//! authentication through the `x-api-key` header. The API may be served over
//! HTTP or HTTPS, and Unraid installations often use self-signed certificates,
//! so this client matches the other local appliance clients and disables cert
//! verification.

use anyhow::Context;
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async_tls_with_config, Connector};

use crate::config::UnraidConfig;

#[derive(Clone)]
pub struct UnraidClient {
    pub name: String,
    endpoint: String,
    key: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Default)]
pub struct UnraidData {
    pub source_name: String,
    pub server_name: String,
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
    pub cpu_pct: u32,
    pub mem_pct: u32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub temperature_c: Option<f64>,
    pub temperature_status: String,
    pub temperature_name: String,
    pub array: UnraidArrayData,
    pub containers: Vec<UnraidContainer>,
    pub vms: Vec<UnraidVm>,
    pub notifications: Vec<UnraidNotification>,
}

#[derive(Debug, Clone, Default)]
pub struct UnraidArrayData {
    pub state: String,
    pub used_kb: u64,
    pub total_kb: u64,
    pub parity: UnraidParity,
    pub disks: Vec<UnraidDisk>,
}

#[derive(Debug, Clone, Default)]
pub struct UnraidParity {
    pub status: String,
    pub progress: u32,
    pub errors: u32,
    pub running: bool,
    pub paused: bool,
    pub speed: String,
}

#[derive(Debug, Clone, Default)]
pub struct UnraidDisk {
    pub id: String,
    pub name: String,
    pub device: String,
    pub kind: String,
    pub status: String,
    pub temp: Option<i32>,
    pub size_kb: u64,
    pub used_kb: u64,
    pub is_spinning: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct UnraidContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub cpu_pct: u32,
    pub mem_pct: u32,
    pub mem_usage: String,
    pub net_io: String,
    pub block_io: String,
    pub auto_start: bool,
    pub update_available: bool,
    pub update_status: String,
    pub size_root_fs: Option<u64>,
    pub size_rw: Option<u64>,
    pub size_log: Option<u64>,
    pub network_mode: String,
    pub lan_ports: Vec<String>,
    pub is_orphaned: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UnraidVm {
    pub id: String,
    pub name: String,
    pub state: String,
}

#[derive(Debug, Clone, Default)]
pub struct UnraidNotification {
    pub title: String,
    pub importance: String,
    pub timestamp: String,
}

#[derive(Deserialize)]
struct GqlResp<T> {
    data: Option<T>,
    errors: Option<Vec<GqlError>>,
}

#[derive(Debug, Deserialize)]
struct GqlError {
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawData {
    server: Option<RawServer>,
    info: RawInfo,
    metrics: Option<RawMetrics>,
    array: RawArray,
    docker: RawDocker,
    vms: RawVms,
    notifications: Option<RawNotifications>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawServer {
    name: Option<String>,
    status: Option<String>,
    lanip: Option<String>,
    localurl: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawInfo {
    os: RawOs,
    cpu: RawCpu,
    versions: RawVersions,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawOs {
    release: Option<String>,
    uptime: Option<String>,
    hostname: Option<String>,
    kernel: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCpu {
    brand: Option<String>,
    cores: Option<u32>,
    threads: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawVersions {
    core: RawCoreVersions,
    packages: Option<RawPackageVersions>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCoreVersions {
    unraid: Option<String>,
    api: Option<String>,
    kernel: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPackageVersions {
    docker: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMetrics {
    cpu: Option<RawCpuMetrics>,
    memory: Option<RawMemoryMetrics>,
    temperature: Option<RawTemperatureMetrics>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCpuMetrics {
    percent_total: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMemoryMetrics {
    total: Value,
    used: Value,
    percent_total: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTemperatureMetrics {
    summary: RawTemperatureSummary,
    #[serde(default)]
    sensors: Vec<RawTemperatureSensor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTemperatureSummary {
    hottest: RawTemperatureSensor,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTemperatureSensor {
    name: String,
    current: RawTemperatureReading,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTemperatureReading {
    value: f64,
    unit: Option<String>,
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawArray {
    state: String,
    capacity: RawArrayCapacity,
    parity_check_status: RawParity,
    #[serde(default)]
    disks: Vec<RawArrayDisk>,
    #[serde(default)]
    caches: Vec<RawArrayDisk>,
    #[serde(default)]
    parities: Vec<RawArrayDisk>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawArrayCapacity {
    kilobytes: RawCapacity,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCapacity {
    used: String,
    total: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawParity {
    status: String,
    progress: Option<u32>,
    errors: Option<u32>,
    running: Option<bool>,
    paused: Option<bool>,
    speed: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawArrayDisk {
    id: String,
    idx: u32,
    name: Option<String>,
    device: Option<String>,
    status: Option<String>,
    temp: Option<i32>,
    fs_size: Option<Value>,
    fs_free: Option<Value>,
    fs_used: Option<Value>,
    size: Option<Value>,
    #[serde(rename = "type")]
    kind: String,
    is_spinning: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDocker {
    #[serde(default)]
    containers: Vec<RawContainer>,
    #[serde(default)]
    container_update_statuses: Vec<RawContainerUpdateStatus>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawContainer {
    id: String,
    #[serde(default)]
    names: Vec<String>,
    image: String,
    state: String,
    status: String,
    auto_start: bool,
    is_update_available: Option<bool>,
    is_rebuild_ready: Option<bool>,
    size_root_fs: Option<Value>,
    size_rw: Option<Value>,
    size_log: Option<Value>,
    lan_ip_ports: Option<Vec<String>>,
    host_config: Option<RawContainerHostConfig>,
    is_orphaned: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawContainerHostConfig {
    network_mode: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawContainerUpdateStatus {
    name: String,
    update_status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawVms {
    domains: Option<Vec<RawVm>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawVm {
    id: String,
    name: Option<String>,
    state: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawNotifications {
    warnings_and_alerts: Vec<RawNotification>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawNotification {
    title: String,
    importance: String,
    timestamp: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawContainerStats {
    id: String,
    cpu_percent: f64,
    mem_usage: String,
    mem_percent: f64,
    #[serde(rename = "netIO")]
    net_io: String,
    #[serde(rename = "blockIO")]
    block_io: String,
}

#[derive(Debug, Deserialize)]
struct GqlWsPayload {
    data: Option<GqlWsData>,
    errors: Option<Vec<GqlError>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlWsData {
    docker_container_stats: RawContainerStats,
}

#[derive(Debug)]
struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

impl UnraidClient {
    pub fn new(cfg: &UnraidConfig, http_timeout_sec: u64) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(http_timeout_sec.max(1)))
            .build()
            .context("building Unraid HTTP client")?;
        Ok(Self {
            name: cfg.name.clone(),
            endpoint: graphql_endpoint(&cfg.host),
            key: cfg.api_key.clone(),
            http,
        })
    }

    async fn graphql<T: serde::de::DeserializeOwned>(&self, query: &str) -> anyhow::Result<T> {
        let resp = self
            .http
            .post(&self.endpoint)
            .header("x-api-key", &self.key)
            .header("Accept", "application/json")
            .json(&json!({ "query": query }))
            .send()
            .await
            .with_context(|| format!("requesting {}", self.endpoint))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!(
                "{} returned HTTP {status}: {}",
                self.endpoint,
                body.chars().take(200).collect::<String>()
            );
        }
        let parsed: GqlResp<T> =
            serde_json::from_str(&body).with_context(|| format!("decoding {}", self.endpoint))?;
        if let Some(errors) = parsed.errors {
            let msg = errors
                .into_iter()
                .map(|e| e.message)
                .collect::<Vec<_>>()
                .join("; ");
            if parsed.data.is_none() {
                anyhow::bail!("GraphQL error from {}: {msg}", self.endpoint);
            }
            tracing::debug!("partial GraphQL response from {}: {msg}", self.endpoint);
        }
        parsed
            .data
            .ok_or_else(|| anyhow::anyhow!("{} returned no GraphQL data", self.endpoint))
    }

    pub async fn collect(&self) -> anyhow::Result<UnraidData> {
        let (raw, stats) =
            futures::join!(self.graphql::<RawData>(COLLECT_QUERY), self.container_stats());
        let mut data = raw?.into_data(&self.name, &self.endpoint);
        match stats {
            Ok(stats) => merge_container_stats(&mut data.containers, stats),
            Err(e) => tracing::debug!("Unraid Docker stats unavailable for '{}': {e:#}", self.name),
        }
        Ok(data)
    }

    async fn container_stats(&self) -> anyhow::Result<BTreeMap<String, RawContainerStats>> {
        let ws_endpoint = websocket_endpoint(&self.endpoint);
        let mut request = ws_endpoint
            .into_client_request()
            .context("building Unraid Docker stats websocket request")?;
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            HeaderValue::from_static("graphql-transport-ws"),
        );
        request.headers_mut().insert(
            "x-api-key",
            HeaderValue::from_str(&self.key).context("building x-api-key websocket header")?,
        );

        let connector = Connector::Rustls(Arc::new(
            ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoCertificateVerification))
                .with_no_client_auth(),
        ));
        let (mut ws, _) =
            connect_async_tls_with_config(request, None, false, Some(connector)).await?;

        ws.send(Message::Text(
            json!({
                "type": "connection_init",
                "payload": { "x-api-key": self.key }
            })
            .to_string()
            .into(),
        ))
        .await?;

        let mut subscribed = false;
        let mut stats = BTreeMap::new();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(1600);

        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            let Some(msg) = tokio::time::timeout(deadline - now, ws.next()).await.ok().flatten()
            else {
                break;
            };
            let msg = msg?;
            let Message::Text(text) = msg else {
                continue;
            };
            let value: Value = serde_json::from_str(&text)?;
            match value.get("type").and_then(Value::as_str) {
                Some("connection_ack") if !subscribed => {
                    subscribed = true;
                    ws.send(Message::Text(
                        json!({
                            "id": "docker-stats",
                            "type": "subscribe",
                            "payload": {
                                "query": "subscription SentinelDockerStats { dockerContainerStats { id cpuPercent memUsage memPercent netIO blockIO } }"
                            }
                        })
                        .to_string()
                        .into(),
                    ))
                    .await?;
                }
                Some("next") => {
                    let payload = value
                        .get("payload")
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("Docker stats websocket frame missing payload"))?;
                    let payload: GqlWsPayload = serde_json::from_value(payload)?;
                    if let Some(errors) = payload.errors {
                        let msg = errors
                            .into_iter()
                            .map(|e| e.message)
                            .collect::<Vec<_>>()
                            .join("; ");
                        anyhow::bail!("Docker stats subscription error: {msg}");
                    }
                    if let Some(data) = payload.data {
                        let mut stat = data.docker_container_stats;
                        stat.id = sanitize_graphql_id(&stat.id);
                        stats.insert(stat.id.clone(), stat);
                    }
                }
                Some("error") => anyhow::bail!("Docker stats websocket returned {value}"),
                _ => {}
            }
        }

        let _ = ws
            .send(Message::Text(
                json!({ "id": "docker-stats", "type": "complete" })
                    .to_string()
                    .into(),
            ))
            .await;
        let _ = ws.close(None).await;
        Ok(stats)
    }
}

impl RawData {
    fn into_data(self, source_name: &str, endpoint: &str) -> UnraidData {
        let server = self.server;
        let server_name = server
            .as_ref()
            .and_then(|s| s.name.clone())
            .or_else(|| self.info.os.hostname.clone())
            .unwrap_or_else(|| source_name.to_string());
        let metrics = self.metrics;
        let cpu_pct = metrics
            .as_ref()
            .and_then(|m| m.cpu.as_ref())
            .map(|c| c.percent_total.round().clamp(0.0, 100.0) as u32)
            .unwrap_or(0);
        let (mem_used, mem_total, mem_pct) = metrics
            .as_ref()
            .and_then(|m| m.memory.as_ref())
            .map(|m| {
                (
                    value_u64(&m.used),
                    value_u64(&m.total),
                    m.percent_total.round().clamp(0.0, 100.0) as u32,
                )
            })
            .unwrap_or((0, 0, 0));
        let (temperature_c, temperature_status, temperature_name) = metrics
            .as_ref()
            .and_then(|m| m.temperature.as_ref())
            .map(temperature_from_metrics)
            .unwrap_or((None, "UNKNOWN".to_string(), String::new()));

        let array = self.array.into_data();
        let docker = self.docker;
        let update_statuses: BTreeMap<String, String> = docker
            .container_update_statuses
            .into_iter()
            .map(|s| (container_key(&s.name), s.update_status))
            .collect();
        let containers = docker
            .containers
            .into_iter()
            .map(|c| {
                let name = c
                    .names
                    .first()
                    .map(|n| n.trim_start_matches('/').to_string())
                    .unwrap_or_else(|| "unnamed".to_string());
                let update_status = update_statuses
                    .get(&container_key(&name))
                    .cloned()
                    .unwrap_or_else(|| {
                        fallback_update_status(c.is_update_available, c.is_rebuild_ready)
                    });
                let update_available =
                    matches!(update_status.as_str(), "UPDATE_AVAILABLE" | "REBUILD_READY")
                        || c.is_update_available.unwrap_or(false)
                        || c.is_rebuild_ready.unwrap_or(false);
                UnraidContainer {
                    id: c.id,
                    name,
                    image: c.image,
                    state: c.state,
                    status: c.status,
                    cpu_pct: 0,
                    mem_pct: 0,
                    mem_usage: "—".to_string(),
                    net_io: "—".to_string(),
                    block_io: "—".to_string(),
                    auto_start: c.auto_start,
                    update_available,
                    update_status,
                    size_root_fs: c.size_root_fs.as_ref().map(value_u64),
                    size_rw: c.size_rw.as_ref().map(value_u64),
                    size_log: c.size_log.as_ref().map(value_u64),
                    network_mode: c
                        .host_config
                        .and_then(|h| h.network_mode)
                        .unwrap_or_default(),
                    lan_ports: c.lan_ip_ports.unwrap_or_default(),
                    is_orphaned: c.is_orphaned,
                }
            })
            .collect();
        let vms = self
            .vms
            .domains
            .unwrap_or_default()
            .into_iter()
            .map(|v| UnraidVm {
                id: v.id,
                name: v.name.unwrap_or_else(|| "unnamed".to_string()),
                state: v.state,
            })
            .collect();
        let notifications = self
            .notifications
            .map(|n| {
                n.warnings_and_alerts
                    .into_iter()
                    .take(20)
                    .map(|x| UnraidNotification {
                        title: x.title,
                        importance: x.importance,
                        timestamp: x.timestamp.unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let kernel = self
            .info
            .versions
            .core
            .kernel
            .or(self.info.os.kernel)
            .unwrap_or_default();
        let version = self
            .info
            .versions
            .core
            .unraid
            .or(self.info.os.release)
            .unwrap_or_default();
        let api_version = self.info.versions.core.api.unwrap_or_default();
        let _docker_version = self.info.versions.packages.and_then(|p| p.docker);

        UnraidData {
            source_name: source_name.to_string(),
            server_name,
            host: endpoint.trim_end_matches("/graphql").to_string(),
            status: server
                .as_ref()
                .and_then(|s| s.status.clone())
                .unwrap_or_else(|| "ONLINE".to_string()),
            lan_ip: server
                .as_ref()
                .and_then(|s| s.lanip.clone())
                .unwrap_or_default(),
            local_url: server
                .as_ref()
                .and_then(|s| s.localurl.clone())
                .unwrap_or_default(),
            version,
            api_version,
            kernel,
            uptime: fmt_boot_age(self.info.os.uptime.as_deref()),
            cpu_brand: self.info.cpu.brand.unwrap_or_default(),
            cpu_cores: self.info.cpu.cores.unwrap_or(0),
            cpu_threads: self.info.cpu.threads.unwrap_or(0),
            cpu_pct,
            mem_pct,
            mem_used,
            mem_total,
            temperature_c,
            temperature_status,
            temperature_name,
            array,
            containers,
            vms,
            notifications,
        }
    }
}

impl RawArray {
    fn into_data(self) -> UnraidArrayData {
        let mut all_disks = Vec::new();
        all_disks.extend(self.parities);
        all_disks.extend(self.disks);
        all_disks.extend(self.caches);

        let disks: Vec<UnraidDisk> = all_disks
            .into_iter()
            .map(|d| {
                let size_kb = d
                    .fs_size
                    .as_ref()
                    .map(value_u64)
                    .unwrap_or_else(|| d.size.as_ref().map(value_u64).unwrap_or(0));
                let free_kb = d.fs_free.as_ref().map(value_u64);
                let used_kb = d
                    .fs_used
                    .as_ref()
                    .map(value_u64)
                    .or_else(|| free_kb.map(|free| size_kb.saturating_sub(free)))
                    .unwrap_or(0);
                UnraidDisk {
                    id: d.id,
                    name: d.name.unwrap_or_else(|| format!("disk{}", d.idx)),
                    device: d.device.unwrap_or_default(),
                    kind: d.kind,
                    status: d.status.unwrap_or_default(),
                    temp: d
                        .temp
                        .and_then(|t| normalize_temperature_c(t as f64, Some("CELSIUS")))
                        .map(|t| t.round() as i32),
                    size_kb,
                    used_kb,
                    is_spinning: d.is_spinning,
                }
            })
            .collect();

        let (used_kb, total_kb) = disks
            .iter()
            .filter(|d| d.kind == "DATA")
            .fold((0u64, 0u64), |(used, total), d| {
                (used + d.used_kb, total + d.size_kb)
            });
        let used_kb = used_kb.max(parse_u64_text(&self.capacity.kilobytes.used));
        let total_kb = total_kb.max(parse_u64_text(&self.capacity.kilobytes.total));

        UnraidArrayData {
            state: self.state,
            used_kb,
            total_kb,
            parity: UnraidParity {
                status: self.parity_check_status.status,
                progress: self.parity_check_status.progress.unwrap_or(0),
                errors: self.parity_check_status.errors.unwrap_or(0),
                running: self.parity_check_status.running.unwrap_or(false),
                paused: self.parity_check_status.paused.unwrap_or(false),
                speed: self.parity_check_status.speed.unwrap_or_default(),
            },
            disks,
        }
    }
}

fn graphql_endpoint(host: &str) -> String {
    let h = host.trim().trim_end_matches('/');
    let with_scheme = if h.starts_with("http://") || h.starts_with("https://") {
        h.to_string()
    } else {
        format!("http://{h}")
    };
    if with_scheme.ends_with("/graphql") {
        with_scheme
    } else {
        format!("{with_scheme}/graphql")
    }
}

fn websocket_endpoint(endpoint: &str) -> String {
    if let Some(rest) = endpoint.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = endpoint.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        endpoint.to_string()
    }
}

fn merge_container_stats(
    containers: &mut [UnraidContainer],
    stats: BTreeMap<String, RawContainerStats>,
) {
    for c in containers {
        let Some(s) = stats.get(&sanitize_graphql_id(&c.id)) else {
            continue;
        };
        c.cpu_pct = s.cpu_percent.round().clamp(0.0, 100.0) as u32;
        c.mem_pct = s.mem_percent.round().clamp(0.0, 100.0) as u32;
        c.mem_usage = stat_text(&s.mem_usage);
        c.net_io = stat_text(&s.net_io);
        c.block_io = stat_text(&s.block_io);
    }
}

fn stat_text(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "—".to_string()
    } else {
        value.to_string()
    }
}

fn sanitize_graphql_id(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            while let Some(next) = chars.next() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if !ch.is_control() {
            out.push(ch);
        }
    }
    out
}

fn value_u64(v: &Value) -> u64 {
    match v {
        Value::Number(n) => n
            .as_u64()
            .unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as u64),
        Value::String(s) => parse_u64_text(s),
        _ => 0,
    }
}

fn parse_u64_text(s: &str) -> u64 {
    s.trim().parse::<u64>().unwrap_or(0)
}

fn temperature_from_metrics(t: &RawTemperatureMetrics) -> (Option<f64>, String, String) {
    let mut hottest: Option<(f64, String, String)> = None;
    let mut has_warning = false;
    let mut has_critical = false;

    for sensor in &t.sensors {
        if ignored_temperature_sensor(sensor) {
            continue;
        }
        let Some(temp) =
            normalize_temperature_c(sensor.current.value, sensor.current.unit.as_deref())
        else {
            continue;
        };
        match sensor.current.status.as_str() {
            "CRITICAL" => has_critical = true,
            "WARNING" => has_warning = true,
            _ => {}
        }
        if hottest.as_ref().map(|(h, _, _)| temp > *h).unwrap_or(true) {
            hottest = Some((temp, sensor.current.status.clone(), sensor.name.clone()));
        }
    }

    if hottest.is_none() && !ignored_temperature_sensor(&t.summary.hottest) {
        hottest = normalize_temperature_c(
            t.summary.hottest.current.value,
            t.summary.hottest.current.unit.as_deref(),
        )
        .map(|temp| {
            (
                temp,
                t.summary.hottest.current.status.clone(),
                t.summary.hottest.name.clone(),
            )
        });
    }

    let status = if has_critical {
        "CRITICAL".to_string()
    } else if has_warning {
        "WARNING".to_string()
    } else {
        hottest
            .as_ref()
            .map(|(_, status, _)| status.clone())
            .unwrap_or_else(|| "UNKNOWN".to_string())
    };
    let name = hottest
        .as_ref()
        .map(|(_, _, name)| name.clone())
        .unwrap_or_default();

    (hottest.map(|(temp, _, _)| temp), status, name)
}

fn ignored_temperature_sensor(sensor: &RawTemperatureSensor) -> bool {
    let name = sensor.name.to_ascii_lowercase();
    name.contains("fan") || name.contains("rpm") || name.contains("tach")
}

fn normalize_temperature_c(value: f64, unit: Option<&str>) -> Option<f64> {
    if !value.is_finite() {
        return None;
    }
    let mut v = value;
    if v.abs() > 250.0 {
        v /= 10.0;
    }
    let celsius = match unit.unwrap_or("CELSIUS") {
        "FAHRENHEIT" => (v - 32.0) * 5.0 / 9.0,
        "KELVIN" => v - 273.15,
        "RANKINE" => (v - 491.67) * 5.0 / 9.0,
        _ => v,
    };
    if (-50.0..=120.0).contains(&celsius) {
        Some(celsius)
    } else {
        None
    }
}

fn container_key(name: &str) -> String {
    name.trim_start_matches('/').to_ascii_lowercase()
}

fn fallback_update_status(update_available: Option<bool>, rebuild_ready: Option<bool>) -> String {
    if rebuild_ready.unwrap_or(false) {
        "REBUILD_READY".to_string()
    } else if update_available.unwrap_or(false) {
        "UPDATE_AVAILABLE".to_string()
    } else {
        "UNKNOWN".to_string()
    }
}

fn fmt_boot_age(value: Option<&str>) -> String {
    let Some(v) = value.filter(|s| !s.is_empty()) else {
        return "—".to_string();
    };
    if let Ok(seconds) = v.parse::<u64>() {
        return fmt_uptime(seconds);
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(v) {
        let secs = Utc::now()
            .signed_duration_since(dt.with_timezone(&Utc))
            .num_seconds()
            .max(0) as u64;
        return fmt_uptime(secs);
    }
    v.to_string()
}

fn fmt_uptime(secs: u64) -> String {
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

const COLLECT_QUERY: &str = r#"
query SentinelUnraid {
  server {
    name
    status
    lanip
    localurl
  }
  info {
    os {
      release
      uptime
      hostname
      kernel
    }
    cpu {
      brand
      cores
      threads
    }
    versions {
      core {
        unraid
        api
        kernel
      }
      packages {
        docker
      }
    }
  }
  metrics {
    cpu {
      percentTotal
    }
    memory {
      total
      used
      percentTotal
    }
    temperature {
      sensors {
        name
        current {
          value
          unit
          status
        }
      }
      summary {
        hottest {
          name
          current {
            value
            unit
            status
          }
        }
      }
    }
  }
  array {
    state
    capacity {
      kilobytes {
        used
        total
      }
    }
    parityCheckStatus {
      status
      progress
      errors
      running
      paused
      speed
    }
    parities {
      id
      idx
      name
      device
      status
      temp
      size
      fsSize
      fsFree
      fsUsed
      type
      isSpinning
    }
    disks {
      id
      idx
      name
      device
      status
      temp
      size
      fsSize
      fsFree
      fsUsed
      type
      isSpinning
    }
    caches {
      id
      idx
      name
      device
      status
      temp
      size
      fsSize
      fsFree
      fsUsed
      type
      isSpinning
    }
  }
  docker {
    containers {
      id
      names
      image
      state
      status
      autoStart
      isUpdateAvailable
      isRebuildReady
      sizeRootFs
      sizeRw
      sizeLog
      lanIpPorts
      hostConfig {
        networkMode
      }
      isOrphaned
    }
    containerUpdateStatuses {
      name
      updateStatus
    }
  }
  vms {
    domains {
      id
      name
      state
    }
  }
  notifications {
    warningsAndAlerts {
      title
      importance
      timestamp
    }
  }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_defaults_to_local_graphql_path() {
        assert_eq!(graphql_endpoint("10.10.0.40"), "http://10.10.0.40/graphql");
        assert_eq!(
            graphql_endpoint("https://tower.local/graphql"),
            "https://tower.local/graphql"
        );
    }

    #[test]
    fn temperature_ignores_fan_sensors_reported_as_celsius() {
        let metrics = RawTemperatureMetrics {
            summary: RawTemperatureSummary {
                hottest: sensor("qnap_ec-isa-0000 fan1", 1308.0, "CRITICAL"),
            },
            sensors: vec![
                sensor("qnap_ec-isa-0000 fan1", 1308.0, "CRITICAL"),
                sensor("coretemp-isa-0000 Package id 0", 70.0, "WARNING"),
                sensor("Samsung SSD 870 EVO 1TB", 36.0, "NORMAL"),
            ],
        };

        let (temp, status, name) = temperature_from_metrics(&metrics);

        assert_eq!(temp.map(|t| t.round() as u32), Some(70));
        assert_eq!(status, "WARNING");
        assert_eq!(name, "coretemp-isa-0000 Package id 0");
    }

    #[test]
    fn temperature_normalizes_units_and_rejects_implausible_values() {
        assert_eq!(
            normalize_temperature_c(131.0, Some("FAHRENHEIT")).map(|t| t.round() as u32),
            Some(55)
        );
        assert_eq!(
            normalize_temperature_c(1040.0, Some("FAHRENHEIT")).map(|t| t.round() as u32),
            Some(40)
        );
        assert!(normalize_temperature_c(1308.0, Some("CELSIUS")).is_none());
    }

    fn sensor(name: &str, value: f64, status: &str) -> RawTemperatureSensor {
        RawTemperatureSensor {
            name: name.to_string(),
            current: RawTemperatureReading {
                value,
                unit: Some("CELSIUS".to_string()),
                status: status.to_string(),
            },
        }
    }
}
