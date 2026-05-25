//! Runtime configuration, loaded from the database.
//!
//! Everything that used to live in `config.toml` — monitored sources, their API
//! credentials, the poll interval, the bind address — plus the values that were
//! previously hardcoded (alert thresholds, HTTP timeout, history sizing) and the
//! frontend UI preferences, all come from PostgreSQL now. The poller reloads
//! this on every cycle, so edits made through the Settings page take effect
//! without a restart (the sole exception is `bind`, applied at startup only).

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;

use crate::db;
use crate::network_scanner::NetworkScannerSettings;
use crate::notify::NotificationSettings;

/// Connection details for a UniFi Network controller.
#[derive(Debug, Clone, Hash)]
pub struct UnifiConfig {
    pub host: String,
    pub api_key: String,
}

/// Connection details for a Proxmox VE host or cluster.
#[derive(Debug, Clone, Hash)]
pub struct ProxmoxConfig {
    pub name: String,
    pub host: String,
    pub token_id: String,
    pub token_secret: String,
}

/// Connection details for an Unraid GraphQL API endpoint.
#[derive(Debug, Clone, Hash)]
pub struct UnraidConfig {
    pub name: String,
    pub host: String,
    pub api_key: String,
}

/// Threshold values that drive the alert rules in the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertThresholds {
    pub guest_mem_crit: u32,
    pub guest_mem_warn: u32,
    pub guest_cpu_warn: u32,
    pub guest_disk_warn: u32,
    pub node_mem_crit: u32,
    pub node_mem_warn: u32,
    pub node_cpu_crit: u32,
    pub node_cpu_warn: u32,
    pub node_disk_warn: u32,
    pub unifi_cpu_warn: u32,
    pub unifi_mem_warn: u32,
    pub unraid_cpu_warn: u32,
    pub unraid_mem_warn: u32,
    pub unraid_array_warn: u32,
    pub unraid_disk_warn: u32,
    pub unraid_temp_warn: u32,
    pub unraid_temp_crit: u32,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            guest_mem_crit: 92,
            guest_mem_warn: 85,
            guest_cpu_warn: 88,
            guest_disk_warn: 90,
            node_mem_crit: 90,
            node_mem_warn: 80,
            node_cpu_crit: 92,
            node_cpu_warn: 85,
            node_disk_warn: 90,
            unifi_cpu_warn: 90,
            unifi_mem_warn: 92,
            unraid_cpu_warn: 90,
            unraid_mem_warn: 90,
            unraid_array_warn: 85,
            unraid_disk_warn: 90,
            unraid_temp_warn: 55,
            unraid_temp_crit: 65,
        }
    }
}

/// Frontend display preferences, previously kept in browser localStorage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiPrefs {
    pub accent: String,
    pub density: String,
    pub show_spark: bool,
    #[serde(default = "default_layouts")]
    pub layouts: Value,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            accent: "cyan".to_string(),
            density: "regular".to_string(),
            show_spark: true,
            layouts: default_layouts(),
        }
    }
}

fn default_layouts() -> Value {
    Value::Object(serde_json::Map::new())
}

/// The fully-resolved configuration the backend runs on.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub poll_interval_sec: u64,
    pub bind: String,
    pub http_timeout_sec: u64,
    pub history_max_samples: usize,
    pub history_retention_days: i64,
    pub frontend_poll_ms: u64,
    pub thresholds: AlertThresholds,
    pub ui_prefs: UiPrefs,
    pub notifications: NotificationSettings,
    pub network_scanner: NetworkScannerSettings,
    pub unifi: Option<UnifiConfig>,
    pub proxmox: Vec<ProxmoxConfig>,
    pub unraid: Vec<UnraidConfig>,
}

impl RuntimeConfig {
    /// Assemble the configuration from the `settings` and source tables.
    pub async fn load(pool: &PgPool) -> anyhow::Result<Self> {
        let map = db::get_settings_map(pool).await?;
        let unifi_rows = db::get_unifi_sources(pool).await?;
        let proxmox_rows = db::get_proxmox_sources(pool).await?;
        let unraid_rows = db::get_unraid_sources(pool).await?;

        // The engine drives a single UniFi controller; use the first enabled one.
        let unifi = unifi_rows
            .into_iter()
            .find(|r| r.enabled)
            .map(|r| UnifiConfig {
                host: r.host,
                api_key: r.api_key,
            });
        let proxmox = proxmox_rows
            .into_iter()
            .filter(|r| r.enabled)
            .map(|r| ProxmoxConfig {
                name: r.name,
                host: r.host,
                token_id: r.token_id,
                token_secret: r.token_secret,
            })
            .collect();
        let unraid = unraid_rows
            .into_iter()
            .filter(|r| r.enabled)
            .map(|r| UnraidConfig {
                name: r.name,
                host: r.host,
                api_key: r.api_key,
            })
            .collect();

        let mut notifications: NotificationSettings =
            setting(&map, "notifications", NotificationSettings::default());
        crate::secret::open_notifications(&mut notifications)?;

        Ok(Self {
            poll_interval_sec: setting(&map, "poll_interval_sec", 15),
            bind: setting(&map, "bind", "0.0.0.0:8787".to_string()),
            http_timeout_sec: setting(&map, "http_timeout_sec", 12),
            history_max_samples: setting(&map, "history_max_samples", 6000usize),
            history_retention_days: setting(&map, "history_retention_days", 30i64).max(1),
            frontend_poll_ms: setting(&map, "frontend_poll_ms", 5000),
            thresholds: setting(&map, "alert_thresholds", AlertThresholds::default()),
            ui_prefs: setting(&map, "ui_prefs", UiPrefs::default()),
            notifications,
            network_scanner: setting(
                &map,
                crate::network_scanner::SETTINGS_KEY,
                NetworkScannerSettings::default(),
            ),
            unifi,
            proxmox,
            unraid,
        })
    }

    /// A hash of everything that, when changed, requires the HTTP clients to be
    /// rebuilt — the source set and the HTTP timeout baked into each client.
    pub fn source_sig(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        self.unifi.hash(&mut h);
        self.proxmox.hash(&mut h);
        self.unraid.hash(&mut h);
        self.http_timeout_sec.hash(&mut h);
        h.finish()
    }
}

/// Read one setting, deserializing it into `T`, falling back to `default` when
/// the key is absent or malformed.
fn setting<T: DeserializeOwned>(map: &HashMap<String, Value>, key: &str, default: T) -> T {
    map.get(key)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(default)
}
