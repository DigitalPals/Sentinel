
//! Redfish + IPMI/BMC client support.
//!
//! Redfish is used for inventory/health data. When `ipmitool` is available in
//! the runtime image, Sentinel also gathers classic IPMI sensor readings over
//! RMCP+ (`lanplus`) for temperatures, voltages, fans and chassis power.

use std::process::Stdio;

use anyhow::Context;
use serde::Deserialize;
use tokio::process::Command;

use crate::config::BmcConfig;

#[derive(Clone)]
pub struct BmcClient {
    pub name: String,
    base: String,
    host_for_ipmi: String,
    username: String,
    password: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct BmcData {
    pub source_name: String,
    pub host: String,
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    pub bios_version: String,
    pub power_state: String,
    pub health: String,
    pub processor_count: u32,
    pub memory_gib: u32,
    pub manager_model: String,
    pub manager_firmware: String,
    pub manager_uuid: String,
    pub ipmi_available: bool,
    pub ipmi_power: Option<String>,
    pub ipmi_device: Option<IpmiDeviceInfo>,
    pub sensors: Vec<BmcSensor>,
    pub drives: Vec<BmcDrive>,
}

#[derive(Debug, Clone, Default)]
pub struct IpmiDeviceInfo {
    pub manufacturer: String,
    pub product: String,
    pub firmware: String,
    pub ipmi_version: String,
}

#[derive(Debug, Clone)]
pub struct BmcSensor {
    pub name: String,
    pub kind: String,
    pub status: String,
    pub reading: Option<f64>,
    pub unit: String,
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct BmcDrive {
    pub name: String,
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    pub capacity_bytes: u64,
    pub health: String,
    pub state: String,
}

#[derive(Deserialize)]
struct RedfishCollection {
    #[serde(default, rename = "Members")]
    members: Vec<OdataRef>,
}

#[derive(Deserialize)]
struct OdataRef {
    #[serde(rename = "@odata.id")]
    id: String,
}

#[derive(Deserialize, Default)]
struct RedfishStatus {
    #[serde(default, rename = "Health")]
    health: Option<String>,
    #[serde(default, rename = "HealthRollup")]
    health_rollup: Option<String>,
    #[serde(default, rename = "State")]
    state: Option<String>,
}

#[derive(Deserialize, Default)]
struct SystemSummary {
    #[serde(default, rename = "Count")]
    count: Option<u32>,
    #[serde(default, rename = "Status")]
    status: RedfishStatus,
}

#[derive(Deserialize, Default)]
struct MemorySummary {
    #[serde(default, rename = "TotalSystemMemoryGiB")]
    total_system_memory_gib: Option<f64>,
    #[serde(default, rename = "Status")]
    status: RedfishStatus,
}

#[derive(Deserialize)]
struct RedfishSystem {
    #[serde(default, rename = "Manufacturer")]
    manufacturer: String,
    #[serde(default, rename = "Model")]
    model: String,
    #[serde(default, rename = "SerialNumber")]
    serial_number: String,
    #[serde(default, rename = "BiosVersion")]
    bios_version: String,
    #[serde(default, rename = "PowerState")]
    power_state: String,
    #[serde(default, rename = "Status")]
    status: RedfishStatus,
    #[serde(default, rename = "ProcessorSummary")]
    processor_summary: SystemSummary,
    #[serde(default, rename = "MemorySummary")]
    memory_summary: MemorySummary,
    #[serde(default, rename = "Storage")]
    storage: Option<OdataRef>,
}

#[derive(Deserialize)]
struct RedfishManager {
    #[serde(default, rename = "Model")]
    model: String,
    #[serde(default, rename = "FirmwareVersion")]
    firmware_version: String,
    #[serde(default, rename = "UUID")]
    uuid: String,
}

#[derive(Deserialize)]
struct RedfishStorage {
    #[serde(default, rename = "Drives")]
    drives: Vec<OdataRef>,
}

#[derive(Deserialize)]
struct RedfishDrive {
    #[serde(default, rename = "Name")]
    name: String,
    #[serde(default, rename = "Manufacturer")]
    manufacturer: String,
    #[serde(default, rename = "Model")]
    model: String,
    #[serde(default, rename = "SerialNumber")]
    serial_number: String,
    #[serde(default, rename = "CapacityBytes")]
    capacity_bytes: Option<u64>,
    #[serde(default, rename = "Status")]
    status: RedfishStatus,
}

impl BmcClient {
    pub fn new(cfg: &BmcConfig, http_timeout_sec: u64) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(http_timeout_sec.max(1)))
            .build()
            .context("building BMC HTTP client")?;
        let host = cfg.host.trim_end_matches('/').to_string();
        let base = if host.starts_with("http://") || host.starts_with("https://") {
            host.clone()
        } else {
            format!("https://{host}")
        };
        let host_for_ipmi = base
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or(&host)
            .split(':')
            .next()
            .unwrap_or(&host)
            .to_string();
        Ok(Self {
            name: cfg.name.clone(),
            base,
            host_for_ipmi,
            username: cfg.username.clone(),
            password: cfg.password.clone(),
            http,
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        let url = if path.starts_with("http") {
            path.to_string()
        } else {
            format!("{}{}", self.base, path)
        };
        let resp = self
            .http
            .get(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!(
                "{url} returned HTTP {status}: {}",
                body.chars().take(200).collect::<String>()
            );
        }
        serde_json::from_str(&body).with_context(|| format!("decoding {url}"))
    }

    pub async fn collect(&self) -> anyhow::Result<BmcData> {
        let systems: RedfishCollection = self.get_json("/redfish/v1/Systems").await?;
        let system_path = systems
            .members
            .first()
            .map(|m| m.id.as_str())
            .unwrap_or("/redfish/v1/Systems/Self");
        let managers: RedfishCollection = self.get_json("/redfish/v1/Managers").await?;
        let manager_path = managers
            .members
            .first()
            .map(|m| m.id.as_str())
            .unwrap_or("/redfish/v1/Managers/Self");
        let system: RedfishSystem = self.get_json(system_path).await.context("system resource")?;
        let manager: RedfishManager = self.get_json(manager_path).await.context("manager resource")?;
        let mut drives = Vec::new();
        if let Some(storage_ref) = system.storage.as_ref() {
            if let Ok(storage_collection) = self.get_json::<RedfishCollection>(&storage_ref.id).await {
                for member in storage_collection.members {
                    if let Ok(storage) = self.get_json::<RedfishStorage>(&member.id).await {
                        for drive_ref in storage.drives {
                            if let Ok(drive) = self.get_json::<RedfishDrive>(&drive_ref.id).await {
                                drives.push(BmcDrive {
                                    name: fallback(drive.name, drive_ref.id.clone()),
                                    manufacturer: blank_to_dash(drive.manufacturer),
                                    model: blank_to_dash(drive.model),
                                    serial: blank_to_dash(drive.serial_number),
                                    capacity_bytes: drive.capacity_bytes.unwrap_or(0),
                                    health: drive.status.health.unwrap_or_else(|| "Unknown".to_string()),
                                    state: drive.status.state.unwrap_or_else(|| "Unknown".to_string()),
                                });
                            }
                        }
                    }
                }
            }
        }

        let (ipmi_available, ipmi_power, ipmi_device, sensors) = self.collect_ipmi().await;

        let health = system
            .status
            .health_rollup
            .or(system.status.health)
            .or(system.processor_summary.status.health_rollup)
            .or(system.memory_summary.status.health_rollup)
            .unwrap_or_else(|| "Unknown".to_string());

        Ok(BmcData {
            source_name: self.name.clone(),
            host: self.host_for_ipmi.clone(),
            manufacturer: blank_to_dash(system.manufacturer),
            model: blank_to_dash(system.model),
            serial: blank_to_dash(system.serial_number),
            bios_version: blank_to_dash(system.bios_version),
            power_state: blank_to_dash(system.power_state),
            health,
            processor_count: system.processor_summary.count.unwrap_or(0),
            memory_gib: system.memory_summary.total_system_memory_gib.unwrap_or(0.0).round() as u32,
            manager_model: blank_to_dash(manager.model),
            manager_firmware: blank_to_dash(manager.firmware_version),
            manager_uuid: blank_to_dash(manager.uuid),
            ipmi_available,
            ipmi_power,
            ipmi_device,
            sensors,
            drives,
        })
    }

    async fn collect_ipmi(&self) -> (bool, Option<String>, Option<IpmiDeviceInfo>, Vec<BmcSensor>) {
        let power = self.run_ipmitool(&["chassis", "power", "status"]).await.ok();
        let info = self
            .run_ipmitool(&["mc", "info"])
            .await
            .ok()
            .map(|s| parse_mc_info(&s));
        let sensors = self
            .run_ipmitool(&["sdr", "elist"])
            .await
            .map(|s| parse_sdr(&s))
            .unwrap_or_default();
        let available = power.is_some() || info.is_some() || !sensors.is_empty();
        (available, power.map(|p| p.trim().to_string()), info, sensors)
    }

    async fn run_ipmitool(&self, args: &[&str]) -> anyhow::Result<String> {
        let output = Command::new("ipmitool")
            .arg("-I")
            .arg("lanplus")
            .arg("-H")
            .arg(&self.host_for_ipmi)
            .arg("-U")
            .arg(&self.username)
            .arg("-E")
            .args(args)
            .env("IPMI_PASSWORD", &self.password)
            .stdin(Stdio::null())
            .output()
            .await
            .context("running ipmitool")?;
        if !output.status.success() {
            anyhow::bail!(
                "ipmitool failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

fn blank_to_dash(s: String) -> String {
    let t = s.trim();
    if t.is_empty() { "—".to_string() } else { t.to_string() }
}

fn fallback(s: String, fb: String) -> String {
    let t = s.trim();
    if t.is_empty() { fb } else { t.to_string() }
}

fn parse_mc_info(text: &str) -> IpmiDeviceInfo {
    let mut info = IpmiDeviceInfo::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim();
        let value = value.trim().to_string();
        match key {
            "Firmware Revision" => info.firmware = value,
            "IPMI Version" => info.ipmi_version = value,
            "Manufacturer Name" => info.manufacturer = value,
            "Product Name" => info.product = value,
            _ => {}
        }
    }
    info
}

fn parse_sdr(text: &str) -> Vec<BmcSensor> {
    text.lines()
        .filter_map(|line| {
            let parts: Vec<_> = line.split('|').map(|p| p.trim()).collect();
            if parts.len() < 5 {
                return None;
            }
            let name = parts[0].to_string();
            let status = parts[2].to_string();
            let raw = parts[4].to_string();
            if raw.eq_ignore_ascii_case("No Reading") || raw.is_empty() {
                return None;
            }
            let (reading, unit) = parse_reading(&raw);
            Some(BmcSensor {
                kind: sensor_kind(&name, &unit),
                name,
                status,
                reading,
                unit,
                raw,
            })
        })
        .collect()
}

fn parse_reading(raw: &str) -> (Option<f64>, String) {
    let mut it = raw.split_whitespace();
    let reading = it.next().and_then(|s| s.parse::<f64>().ok());
    let rest: Vec<&str> = it.collect();
    let unit = if rest.len() >= 2 && rest[0].eq_ignore_ascii_case("degrees") && rest[1] == "C" {
        "°C".to_string()
    } else {
        rest.join(" ")
    };
    (reading, unit)
}

fn sensor_kind(name: &str, unit: &str) -> String {
    let n = name.to_ascii_lowercase();
    let u = unit.to_ascii_lowercase();
    if u.contains("volt") || n.contains("voltage") {
        "voltage".to_string()
    } else if u.contains("rpm") || n.contains("fan") {
        "fan".to_string()
    } else if u.contains("°c") || n.contains("temp") || n.contains("temperature") {
        "temperature".to_string()
    } else if u.contains("watt") || n.contains("power") {
        "power".to_string()
    } else {
        "state".to_string()
    }
}
