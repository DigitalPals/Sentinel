//! UniFi Network local Integration API client (UniFi Network 9.0+).
//!
//! Authenticates with an `X-API-KEY` header against
//! `/proxy/network/integration/v1`. The UniFi OS console serves a self-signed
//! certificate, so verification is disabled.

use anyhow::Context;
use futures::stream::{self, StreamExt};
use serde::Deserialize;

use crate::config::UnifiConfig;

#[derive(Clone)]
pub struct UnifiClient {
    base: String,
    key: String,
    http: reqwest::Client,
}

/// Generic paginated list envelope.
#[derive(Deserialize)]
struct Page<T> {
    data: Vec<T>,
    #[serde(rename = "totalCount", default)]
    total_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Site {
    pub id: String,
    pub name: String,
}

/// Device as returned by the list endpoint (`features`/`interfaces` are arrays).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceListItem {
    pub id: String,
    pub mac_address: Option<String>,
    pub ip_address: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
    pub state: Option<String>,
    pub firmware_version: Option<String>,
    #[serde(default)]
    pub firmware_updatable: bool,
    #[serde(default)]
    pub features: Vec<String>,
}

/// Device as returned by the detail endpoint (`interfaces` is an object).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDetail {
    pub uplink: Option<Uplink>,
    pub interfaces: Option<DetailInterfaces>,
    pub provisioned_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Uplink {
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DetailInterfaces {
    #[serde(default)]
    pub ports: Vec<Port>,
    #[serde(default)]
    pub radios: Vec<Radio>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct Port {
    pub idx: u32,
    pub state: Option<String>,
    pub connector: Option<String>,
    pub speed_mbps: Option<u32>,
    pub max_speed_mbps: Option<u32>,
    pub poe: Option<Poe>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Poe {
    pub state: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Radio {
    #[serde(rename = "frequencyGHz")]
    pub frequency_ghz: Option<f64>,
    pub channel: Option<u32>,
    #[serde(rename = "channelWidthMHz")]
    pub channel_width_mhz: Option<u32>,
    pub wlan_standard: Option<String>,
}

/// Latest live statistics for a device.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStats {
    pub uptime_sec: Option<u64>,
    pub cpu_utilization_pct: Option<f64>,
    pub memory_utilization_pct: Option<f64>,
    pub uplink: Option<StatsUplink>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StatsUplink {
    pub tx_rate_bps: Option<u64>,
    pub rx_rate_bps: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UniClient {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub uplink_device_id: Option<String>,
}

/// Everything gathered about one device in a single poll.
pub struct DeviceBundle {
    pub list: DeviceListItem,
    pub detail: DeviceDetail,
    pub stats: DeviceStats,
}

/// Result of one full UniFi poll.
pub struct UnifiData {
    pub site: String,
    pub app_version: String,
    pub devices: Vec<DeviceBundle>,
    pub clients: Vec<UniClient>,
}

impl UnifiClient {
    pub fn new(cfg: &UnifiConfig) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .context("building UniFi HTTP client")?;
        Ok(Self {
            base: format!(
                "{}/proxy/network/integration/v1",
                cfg.host.trim_end_matches('/')
            ),
            key: cfg.api_key.clone(),
            http,
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .get(&url)
            .header("X-API-KEY", &self.key)
            .header("Accept", "application/json")
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("{url} returned HTTP {status}: {}", body.chars().take(200).collect::<String>());
        }
        serde_json::from_str(&body).with_context(|| format!("decoding {url}"))
    }

    /// Fetch every page of a paginated collection.
    async fn get_all<T: serde::de::DeserializeOwned>(&self, base_path: &str) -> anyhow::Result<Vec<T>> {
        let mut out: Vec<T> = Vec::new();
        let mut offset = 0;
        let limit = 200;
        loop {
            let sep = if base_path.contains('?') { '&' } else { '?' };
            let page: Page<T> = self
                .get_json(&format!("{base_path}{sep}limit={limit}&offset={offset}"))
                .await?;
            let got = page.data.len();
            out.extend(page.data);
            offset += limit;
            if got < limit || (page.total_count > 0 && offset as i64 >= page.total_count) {
                break;
            }
            if offset > 5000 {
                break; // safety valve
            }
        }
        Ok(out)
    }

    pub async fn collect(&self) -> anyhow::Result<UnifiData> {
        #[derive(Deserialize)]
        struct Info {
            #[serde(rename = "applicationVersion")]
            application_version: Option<String>,
        }
        let app_version = self
            .get_json::<Info>("/info")
            .await
            .ok()
            .and_then(|i| i.application_version)
            .unwrap_or_default();

        let sites: Vec<Site> = self.get_all("/sites").await.context("/sites")?;
        let site = sites
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("UniFi returned no sites"))?;

        let list: Vec<DeviceListItem> = self
            .get_all(&format!("/sites/{}/devices", site.id))
            .await
            .context("device list")?;
        let clients: Vec<UniClient> = self
            .get_all(&format!("/sites/{}/clients", site.id))
            .await
            .unwrap_or_default();

        // Detail + statistics for every device, fetched with bounded concurrency.
        let devices: Vec<DeviceBundle> = stream::iter(list.into_iter())
            .map(|item| {
                let this = self.clone();
                let site_id = site.id.clone();
                async move {
                    let detail = this
                        .get_json::<DeviceDetail>(&format!(
                            "/sites/{site_id}/devices/{}",
                            item.id
                        ))
                        .await
                        .unwrap_or_default();
                    let stats = this
                        .get_json::<DeviceStats>(&format!(
                            "/sites/{site_id}/devices/{}/statistics/latest",
                            item.id
                        ))
                        .await
                        .unwrap_or_default();
                    DeviceBundle {
                        list: item,
                        detail,
                        stats,
                    }
                }
            })
            .buffer_unordered(10)
            .collect()
            .await;

        Ok(UnifiData {
            site: site.name,
            app_version,
            devices,
            clients,
        })
    }
}
