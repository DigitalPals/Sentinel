//! UniFi Network local Integration API client (UniFi Network 9.0+).
//!
//! Authenticates with an `X-API-KEY` header against
//! `/proxy/network/integration/v1`. The UniFi OS console serves a self-signed
//! certificate, so verification is disabled.

use std::collections::BTreeMap;

use anyhow::Context;
use futures::stream::{self, StreamExt};
use serde::Deserialize;
use serde_json::Value;

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
    #[serde(default, rename = "internalReference")]
    pub internal_reference: Option<String>,
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
    #[serde(default, alias = "uplinkDeviceId")]
    pub device_id: Option<String>,
    #[serde(default, alias = "uplinkDeviceName")]
    pub device_name: Option<String>,
    #[serde(default, alias = "portIdx", alias = "port", alias = "switchPort")]
    pub port_idx: Option<u32>,
    #[serde(default, alias = "portName", alias = "switchPortName")]
    pub port_name: Option<String>,
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
    #[serde(default, alias = "_id")]
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(default, alias = "mac")]
    pub mac_address: Option<String>,
    #[serde(default, alias = "ip")]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub network_name: Option<String>,
    #[serde(default, alias = "essid")]
    pub ssid: Option<String>,
    #[serde(default)]
    pub connected_at: Option<String>,
    #[serde(default)]
    pub last_seen_at: Option<String>,
    #[serde(default, alias = "uplinkDeviceID")]
    pub uplink_device_id: Option<String>,
    #[serde(default)]
    pub uplink_device_name: Option<String>,
    #[serde(default, alias = "portIdx", alias = "swPort", alias = "switchPort")]
    pub uplink_port: Option<u32>,
    #[serde(default, alias = "switchPortName")]
    pub uplink_port_name: Option<String>,
    #[serde(default)]
    pub rx_rate_bps: Option<u64>,
    #[serde(default)]
    pub tx_rate_bps: Option<u64>,
    #[serde(default)]
    pub rx_bytes: Option<u64>,
    #[serde(default)]
    pub tx_bytes: Option<u64>,
    #[serde(default)]
    pub signal: Option<i64>,
    #[serde(default)]
    pub channel: Option<u32>,
    #[serde(default)]
    pub vlan_id: Option<u32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegacyClient {
    #[serde(default, rename = "_id")]
    pub id: Option<String>,
    #[serde(default)]
    pub mac: Option<String>,
    #[serde(default, alias = "ipAddress")]
    pub ip: Option<String>,
    #[serde(default)]
    pub fixed_ip: Option<String>,
    #[serde(default)]
    pub last_ip: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub last_connection_network_name: Option<String>,
    #[serde(default)]
    pub gw_vlan: Option<u32>,
    #[serde(default)]
    pub vlan: Option<u32>,
    #[serde(default)]
    pub is_wired: Option<bool>,
    #[serde(default)]
    pub first_seen: Option<i64>,
    #[serde(default)]
    pub last_seen: Option<i64>,
    #[serde(default)]
    pub sw_mac: Option<String>,
    #[serde(default)]
    pub sw_port: Option<u32>,
    #[serde(default)]
    pub wired_rate_mbps: Option<u32>,
    #[serde(default)]
    pub last_uplink_name: Option<String>,
    #[serde(default)]
    pub last_uplink_mac: Option<String>,
    #[serde(default)]
    pub last_uplink_remote_port: Option<u32>,
    #[serde(default, rename = "wired-tx_bytes")]
    pub wired_tx_bytes: Option<u64>,
    #[serde(default, rename = "wired-rx_bytes")]
    pub wired_rx_bytes: Option<u64>,
    #[serde(default, rename = "wired-tx_bytes-r")]
    pub wired_tx_bytes_rate: Option<f64>,
    #[serde(default, rename = "wired-rx_bytes-r")]
    pub wired_rx_bytes_rate: Option<f64>,
    #[serde(default)]
    pub tx_bytes: Option<u64>,
    #[serde(default)]
    pub rx_bytes: Option<u64>,
    #[serde(default, rename = "tx_bytes-r")]
    pub tx_bytes_rate: Option<f64>,
    #[serde(default, rename = "rx_bytes-r")]
    pub rx_bytes_rate: Option<f64>,
    #[serde(default)]
    pub signal: Option<i64>,
    #[serde(default)]
    pub channel: Option<u32>,
}

#[derive(Deserialize)]
struct LegacyClientEnvelope {
    #[serde(default)]
    data: Vec<LegacyClient>,
}

/// Everything gathered about one device in a single poll.
pub struct DeviceBundle {
    pub list: DeviceListItem,
    pub detail: DeviceDetail,
    pub stats: DeviceStats,
}

/// Result of one full UniFi poll.
pub struct UnifiData {
    pub site_id: String,
    pub site_reference: String,
    pub site: String,
    pub app_version: String,
    pub devices: Vec<DeviceBundle>,
    pub clients: Vec<UniClient>,
}

impl UnifiClient {
    pub fn new(cfg: &UnifiConfig, http_timeout_sec: u64) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(http_timeout_sec.max(1)))
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
            anyhow::bail!(
                "{url} returned HTTP {status}: {}",
                body.chars().take(200).collect::<String>()
            );
        }
        serde_json::from_str(&body).with_context(|| format!("decoding {url}"))
    }

    /// Fetch every page of a paginated collection.
    async fn get_all<T: serde::de::DeserializeOwned>(
        &self,
        base_path: &str,
    ) -> anyhow::Result<Vec<T>> {
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
                        .get_json::<DeviceDetail>(&format!("/sites/{site_id}/devices/{}", item.id))
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
            site_id: site.id,
            site_reference: site
                .internal_reference
                .clone()
                .unwrap_or_else(|| site.name.clone()),
            site: site.name,
            app_version,
            devices,
            clients,
        })
    }

    pub async fn client_detail(&self, site_id: &str, client_id: &str) -> anyhow::Result<UniClient> {
        self.get_json::<UniClient>(&format!("/sites/{site_id}/clients/{client_id}"))
            .await
            .context("client detail")
    }

    pub async fn legacy_clients(&self, site_reference: &str) -> anyhow::Result<Vec<LegacyClient>> {
        let base = self
            .base
            .strip_suffix("/proxy/network/integration/v1")
            .unwrap_or(&self.base);
        let path = format!("/proxy/network/api/s/{site_reference}/stat/sta");
        let url = format!("{base}{path}");
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
            anyhow::bail!(
                "{url} returned HTTP {status}: {}",
                body.chars().take(200).collect::<String>()
            );
        }
        let envelope: LegacyClientEnvelope =
            serde_json::from_str(&body).with_context(|| format!("decoding {url}"))?;
        Ok(envelope.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_wired_client_switch_port_fields() {
        let json = r#"{
          "data": [{
            "_id": "647329e511675c0098d3b9c5",
            "ip": "10.10.0.3",
            "last_ip": "10.10.0.3",
            "fixed_ip": "10.10.0.3",
            "mac": "28:9c:6e:59:fe:f6",
            "name": "Modbus-Generator",
            "network": "Default",
            "last_connection_network_name": "Default",
            "gw_vlan": 1,
            "is_wired": true,
            "first_seen": 1685268965,
            "last_seen": 1779694335,
            "last_uplink_name": "Schuilstal",
            "last_uplink_mac": "24:5a:4c:15:dd:5b",
            "last_uplink_remote_port": 13,
            "sw_mac": "24:5a:4c:15:dd:5b",
            "sw_port": 13,
            "wired_rate_mbps": 10,
            "wired-tx_bytes": 3272419325,
            "wired-rx_bytes": 62527001,
            "wired-tx_bytes-r": 2.6800889005098707,
            "wired-rx_bytes-r": 2.1571447248006277
          }]
        }"#;

        let envelope: LegacyClientEnvelope = serde_json::from_str(json).unwrap();
        let client = &envelope.data[0];
        assert_eq!(client.ip.as_deref(), Some("10.10.0.3"));
        assert_eq!(client.last_ip.as_deref(), Some("10.10.0.3"));
        assert_eq!(client.fixed_ip.as_deref(), Some("10.10.0.3"));
        assert_eq!(client.network.as_deref(), Some("Default"));
        assert_eq!(client.last_connection_network_name.as_deref(), Some("Default"));
        assert_eq!(client.gw_vlan, Some(1));
        assert_eq!(client.last_uplink_name.as_deref(), Some("Schuilstal"));
        assert_eq!(client.sw_mac.as_deref(), Some("24:5a:4c:15:dd:5b"));
        assert_eq!(client.sw_port, Some(13));
        assert_eq!(client.wired_rate_mbps, Some(10));
        assert_eq!(client.wired_tx_bytes, Some(3_272_419_325));
        assert_eq!(client.wired_rx_bytes, Some(62_527_001));
        assert_eq!(client.is_wired, Some(true));
    }

    #[test]
    fn parses_integration_site_internal_reference() {
        let site: Site = serde_json::from_str(
            r#"{"id":"site-id","internalReference":"default","name":"Default"}"#,
        )
        .unwrap();
        assert_eq!(site.internal_reference.as_deref(), Some("default"));
    }
}
