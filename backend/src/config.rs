//! Configuration loading for Cybex Sentinel.
//!
//! Reads `config.toml` (override path with the `SENTINEL_CONFIG` env var).

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_poll")]
    pub poll_interval_sec: u64,
    #[serde(default = "default_bind")]
    pub bind: String,
    pub unifi: Option<UnifiConfig>,
    #[serde(default)]
    pub proxmox: Vec<ProxmoxConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnifiConfig {
    pub host: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProxmoxConfig {
    pub name: String,
    pub host: String,
    pub token_id: String,
    pub token_secret: String,
}

fn default_poll() -> u64 {
    15
}

fn default_bind() -> String {
    "0.0.0.0:8787".to_string()
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let path = std::env::var("SENTINEL_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
        let text = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("could not read config file '{path}': {e}"))?;
        let cfg: Config = toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("could not parse config file '{path}': {e}"))?;
        if cfg.unifi.is_none() && cfg.proxmox.is_empty() {
            anyhow::bail!("config has no [unifi] section and no [[proxmox]] entries");
        }
        Ok(cfg)
    }
}
