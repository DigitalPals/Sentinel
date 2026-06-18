//! Proxmox Backup Server API client.
//!
//! PBS uses the same `/api2/json` envelope style as PVE, but its API-token
//! header has a different scheme and separator: `PBSAPIToken=token-id:secret`.

use anyhow::Context;
use serde::Deserialize;

use crate::config::PbsConfig;

#[derive(Clone)]
pub struct PbsClient {
    pub name: String,
    base: String,
    auth: String,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct PbsResp<T> {
    data: T,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PbsDatastore {
    pub store: String,
    #[serde(default)]
    pub used: Option<u64>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub avail: Option<u64>,
    #[serde(default, rename = "gc-status")]
    pub gc_status: Option<String>,
    #[serde(default, rename = "maintenance-mode")]
    pub maintenance_mode: Option<String>,
}

impl PbsDatastore {
    pub fn used_pct(&self) -> Option<u32> {
        let total = self.total?;
        if total == 0 {
            return None;
        }
        Some(((self.used.unwrap_or(0) as f64 / total as f64) * 100.0).floor() as u32)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PbsNamespace {
    #[serde(default)]
    pub ns: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PbsGroup {
    #[serde(default, skip_deserializing)]
    pub datastore: String,
    #[serde(default, skip_deserializing)]
    pub namespace: String,
    #[serde(rename = "backup-type")]
    pub backup_type: String,
    #[serde(rename = "backup-id")]
    pub backup_id: String,
    #[serde(default, rename = "backup-count")]
    pub backup_count: u32,
    #[serde(default, rename = "last-backup")]
    pub last_backup: Option<i64>,
    #[serde(default)]
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PbsVerification {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub upid: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PbsSnapshot {
    #[serde(default, skip_deserializing)]
    pub datastore: String,
    #[serde(default, skip_deserializing)]
    pub namespace: String,
    #[serde(rename = "backup-type")]
    pub backup_type: String,
    #[serde(rename = "backup-id")]
    pub backup_id: String,
    #[serde(rename = "backup-time")]
    pub backup_time: i64,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub protected: bool,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub verification: Option<PbsVerification>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PbsTask {
    pub upid: String,
    #[serde(default)]
    pub node: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default, rename = "worker_type")]
    pub worker_type: String,
    #[serde(default, rename = "worker_id")]
    pub worker_id: Option<String>,
    pub starttime: i64,
    #[serde(default)]
    pub endtime: Option<i64>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PbsData {
    pub server: String,
    pub version: String,
    pub datastores: Vec<PbsDatastore>,
    pub groups: Vec<PbsGroup>,
    pub snapshots: Vec<PbsSnapshot>,
    pub tasks: Vec<PbsTask>,
}

impl PbsData {
    pub fn recent_snapshots(&self, limit: usize) -> Vec<PbsSnapshot> {
        let mut snaps = self.snapshots.clone();
        snaps.sort_by(|a, b| b.backup_time.cmp(&a.backup_time));
        snaps.truncate(limit);
        snaps
    }
}

pub fn pbs_api_token_header(token_id: &str, token_secret: &str) -> String {
    format!("PBSAPIToken={}:{}", token_id.trim(), token_secret.trim())
}

impl PbsClient {
    pub fn new(cfg: &PbsConfig, http_timeout_sec: u64) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(http_timeout_sec.max(1)))
            .build()
            .context("building PBS HTTP client")?;
        Ok(Self {
            name: cfg.name.clone(),
            base: cfg.host.trim_end_matches('/').to_string(),
            auth: pbs_api_token_header(&cfg.token_id, &cfg.token_secret),
            http,
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        self.get_json_query(path, &[]).await
    }

    async fn get_json_query<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> anyhow::Result<T> {
        let url = format!("{}/api2/json{}", self.base, path);
        let resp = self
            .http
            .get(&url)
            .query(query)
            .header("Authorization", &self.auth)
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
        let parsed: PbsResp<T> =
            serde_json::from_str(&body).with_context(|| format!("decoding {url}"))?;
        Ok(parsed.data)
    }

    async fn get_json_query_optional<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> anyhow::Result<Vec<T>> {
        match self.get_json_query::<Vec<T>>(path, query).await {
            Ok(data) => Ok(data),
            Err(e) => {
                tracing::warn!(source = %self.name, path, error = %e, "optional PBS endpoint failed");
                Ok(Vec::new())
            }
        }
    }

    pub async fn collect(&self) -> anyhow::Result<PbsData> {
        #[derive(Deserialize)]
        struct Version {
            #[serde(default)]
            version: String,
            #[serde(default)]
            release: String,
        }
        let version: Version = self.get_json("/version").await.context("/version")?;
        let datastores: Vec<PbsDatastore> = self
            .get_json("/admin/datastore")
            .await
            .context("/admin/datastore")?;
        let version = if version.version.is_empty() {
            version.release
        } else {
            version.version
        };
        let mut groups = Vec::new();
        let mut snapshots = Vec::new();
        for store in &datastores {
            let mut namespaces: Vec<PbsNamespace> = self
                .get_json_query_optional(
                    &format!("/admin/datastore/{}/namespace", store.store),
                    &[],
                )
                .await?;
            if namespaces.is_empty() {
                namespaces.push(PbsNamespace { ns: String::new() });
            }
            for ns in namespaces {
                let namespace = ns.ns;
                let query = if namespace.is_empty() {
                    Vec::new()
                } else {
                    vec![("ns".to_string(), namespace.clone())]
                };
                let mut ns_groups: Vec<PbsGroup> = self
                    .get_json_query_optional(
                        &format!("/admin/datastore/{}/groups", store.store),
                        &query,
                    )
                    .await?;
                for g in &mut ns_groups {
                    g.datastore = store.store.clone();
                    g.namespace = namespace.clone();
                }
                groups.extend(ns_groups);
                let mut ns_snapshots: Vec<PbsSnapshot> = self
                    .get_json_query_optional(
                        &format!("/admin/datastore/{}/snapshots", store.store),
                        &query,
                    )
                    .await?;
                for s in &mut ns_snapshots {
                    s.datastore = store.store.clone();
                    s.namespace = namespace.clone();
                }
                snapshots.extend(ns_snapshots);
            }
        }
        let tasks: Vec<PbsTask> = self
            .get_json_query_optional(
                "/nodes/localhost/tasks",
                &[("limit".to_string(), "50".to_string())],
            )
            .await?;
        Ok(PbsData {
            server: self.name.clone(),
            version,
            datastores,
            groups,
            snapshots,
            tasks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_deserializes_verification_and_size() {
        let snap: PbsSnapshot = serde_json::from_str(
            r#"{
            "backup-type":"ct",
            "backup-id":"207",
            "backup-time":1780555336,
            "comment":"openclaw",
            "owner":"pvebackup@pbs!pve",
            "protected":false,
            "size":2279455820,
            "verification":{"state":"ok","upid":"UPID:pbs:verify"}
        }"#,
        )
        .expect("snapshot shape");
        assert_eq!(snap.backup_type, "ct");
        assert_eq!(snap.backup_id, "207");
        assert_eq!(
            snap.verification.as_ref().unwrap().state.as_deref(),
            Some("ok")
        );
        assert_eq!(snap.size, Some(2279455820));
    }

    #[test]
    fn pbs_data_recent_backups_are_sorted_and_include_namespace() {
        let data = PbsData {
            server: "BlackBox".to_string(),
            version: "4.0".to_string(),
            datastores: Vec::new(),
            groups: Vec::new(),
            snapshots: vec![
                PbsSnapshot {
                    datastore: "blackbox".to_string(),
                    namespace: "pve".to_string(),
                    backup_type: "ct".to_string(),
                    backup_id: "101".to_string(),
                    backup_time: 10,
                    comment: Some("old".to_string()),
                    owner: None,
                    protected: false,
                    size: Some(1),
                    verification: None,
                },
                PbsSnapshot {
                    datastore: "blackbox".to_string(),
                    namespace: "pve-dev".to_string(),
                    backup_type: "vm".to_string(),
                    backup_id: "100".to_string(),
                    backup_time: 20,
                    comment: Some("new".to_string()),
                    owner: None,
                    protected: false,
                    size: Some(2),
                    verification: None,
                },
            ],
            tasks: Vec::new(),
        };
        let recent = data.recent_snapshots(2);
        assert_eq!(recent[0].namespace, "pve-dev");
        assert_eq!(recent[0].backup_id, "100");
        assert_eq!(recent[1].backup_id, "101");
    }

    #[test]
    fn pbs_api_token_header_uses_equals_separator() {
        assert_eq!(
            pbs_api_token_header("root@pam!hermes", "s3cret"),
            "PBSAPIToken=root@pam!hermes:s3cret"
        );
    }

    #[test]
    fn datastore_usage_handles_missing_total_without_panic() {
        let store = PbsDatastore {
            store: "blackbox".to_string(),
            used: Some(10),
            total: None,
            avail: Some(90),
            gc_status: None,
            maintenance_mode: None,
        };
        assert_eq!(store.used_pct(), None);
    }

    #[test]
    fn datastore_usage_percent_is_rounded_down() {
        let store = PbsDatastore {
            store: "blackbox".to_string(),
            used: Some(40),
            total: Some(100),
            avail: Some(60),
            gc_status: None,
            maintenance_mode: None,
        };
        assert_eq!(store.used_pct(), Some(40));
    }
}
