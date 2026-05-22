//! Proxmox VE API client.
//!
//! Talks to the `/api2/json` REST API using an API-token `Authorization`
//! header. Proxmox ships a self-signed certificate, so the client is built
//! with certificate verification disabled.

use anyhow::Context;
use serde::Deserialize;

use crate::config::ProxmoxConfig;

/// A configured Proxmox VE endpoint.
#[derive(Clone)]
pub struct ProxmoxClient {
    pub name: String,
    base: String,
    auth: String,
    http: reqwest::Client,
}

/// Envelope every Proxmox endpoint wraps its payload in.
#[derive(Deserialize)]
struct PveResp<T> {
    data: T,
}

/// One row from `/cluster/resources` — covers nodes, VMs, containers, storage.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PveResource {
    #[serde(rename = "type")]
    pub kind: String,
    pub node: Option<String>,
    pub status: Option<String>,
    pub name: Option<String>,
    pub storage: Option<String>,
    pub content: Option<String>,
    pub tags: Option<String>,
    pub vmid: Option<u64>,
    #[serde(default)]
    pub cpu: f64,
    #[serde(default)]
    pub maxcpu: f64,
    #[serde(default)]
    pub mem: u64,
    #[serde(default)]
    pub maxmem: u64,
    #[serde(default)]
    pub disk: u64,
    #[serde(default)]
    pub maxdisk: u64,
    #[serde(default)]
    pub uptime: u64,
}

/// One RRD sample from `/nodes/{node}/rrddata`.
#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct RrdPoint {
    #[serde(default)]
    pub time: i64,
    pub cpu: Option<f64>,
    pub netin: Option<f64>,
    pub netout: Option<f64>,
    pub memused: Option<f64>,
    pub memtotal: Option<f64>,
}

/// A finished or running task from `/nodes/{node}/tasks`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PveTask {
    pub upid: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub status: Option<String>,
    pub node: String,
    pub user: Option<String>,
    pub id: Option<String>,
    pub starttime: i64,
    pub endtime: Option<i64>,
}

/// Result of probing a single Proxmox host.
pub struct ProxmoxData {
    pub server: String,
    pub release: String,
    pub resources: Vec<PveResource>,
    /// node name -> latest non-empty RRD sample
    pub node_rrd: std::collections::HashMap<String, RrdPoint>,
    pub tasks: Vec<PveTask>,
}

impl ProxmoxClient {
    pub fn new(cfg: &ProxmoxConfig) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .context("building Proxmox HTTP client")?;
        Ok(Self {
            name: cfg.name.clone(),
            base: cfg.host.trim_end_matches('/').to_string(),
            auth: format!("PVEAPIToken={}={}", cfg.token_id, cfg.token_secret),
            http,
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        let url = format!("{}/api2/json{}", self.base, path);
        let resp = self
            .http
            .get(&url)
            .header("Authorization", &self.auth)
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("{url} returned HTTP {status}: {}", body.chars().take(200).collect::<String>());
        }
        let parsed: PveResp<T> =
            serde_json::from_str(&body).with_context(|| format!("decoding {url}"))?;
        Ok(parsed.data)
    }

    /// Probe version, cluster resources, per-node RRD network samples and tasks.
    pub async fn collect(&self) -> anyhow::Result<ProxmoxData> {
        #[derive(Deserialize)]
        struct Version {
            release: String,
        }
        let version: Version = self.get_json("/version").await.context("/version")?;
        let resources: Vec<PveResource> = self
            .get_json("/cluster/resources")
            .await
            .context("/cluster/resources")?;

        let node_names: Vec<String> = resources
            .iter()
            .filter(|r| r.kind == "node")
            .filter_map(|r| r.node.clone())
            .collect();

        let mut node_rrd = std::collections::HashMap::new();
        let mut tasks: Vec<PveTask> = Vec::new();
        for node in &node_names {
            if let Ok(points) = self
                .get_json::<Vec<RrdPoint>>(&format!(
                    "/nodes/{node}/rrddata?timeframe=hour&cf=AVERAGE"
                ))
                .await
            {
                if let Some(last) = points
                    .into_iter()
                    .rev()
                    .find(|p| p.netin.is_some() || p.cpu.is_some())
                {
                    node_rrd.insert(node.clone(), last);
                }
            }
            if let Ok(mut t) = self
                .get_json::<Vec<PveTask>>(&format!("/nodes/{node}/tasks?limit=40&source=archive"))
                .await
            {
                tasks.append(&mut t);
            }
        }

        Ok(ProxmoxData {
            server: self.name.clone(),
            release: version.release,
            resources,
            node_rrd,
            tasks,
        })
    }
}
