//! One-time migration of the legacy file-based configuration into the database.
//!
//! Run as `cybex-sentinel import-config`. Reads the old `config.toml` and
//! `data/history.json` and writes their contents into PostgreSQL. It is
//! idempotent — running it twice imports nothing new — so it is safe to run
//! before the legacy files are deleted. This is the only code that still
//! parses TOML; the running backend never reads configuration from a file.

use anyhow::Context;
use serde::Deserialize;
use sqlx::PgPool;

use crate::db;
use crate::history::Sample;

#[derive(Deserialize)]
struct LegacyConfig {
    poll_interval_sec: Option<u64>,
    bind: Option<String>,
    unifi: Option<LegacyUnifi>,
    #[serde(default)]
    proxmox: Vec<LegacyProxmox>,
}

#[derive(Deserialize)]
struct LegacyUnifi {
    host: String,
    api_key: String,
}

#[derive(Deserialize)]
struct LegacyProxmox {
    name: String,
    host: String,
    token_id: String,
    token_secret: String,
}

#[derive(Deserialize)]
struct LegacyHistory {
    #[serde(default)]
    samples: Vec<Sample>,
}

/// Import the legacy config and history files, then return.
pub async fn run(pool: &PgPool) -> anyhow::Result<()> {
    import_config(pool).await?;
    import_history(pool).await?;
    tracing::info!(
        "import-config complete — the legacy config.toml and data/history.json may now be removed"
    );
    Ok(())
}

async fn import_config(pool: &PgPool) -> anyhow::Result<()> {
    let path = std::env::var("SENTINEL_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("no config file at '{path}' ({e}) — skipping config import");
            return Ok(());
        }
    };
    let cfg: LegacyConfig =
        toml::from_str(&text).with_context(|| format!("parsing legacy config '{path}'"))?;

    if let Some(v) = cfg.poll_interval_sec {
        db::set_setting(pool, "poll_interval_sec", &serde_json::json!(v)).await?;
    }
    if let Some(v) = cfg.bind {
        db::set_setting(pool, "bind", &serde_json::json!(v)).await?;
    }

    if let Some(u) = cfg.unifi {
        let exists = db::get_unifi_sources(pool)
            .await?
            .iter()
            .any(|r| r.host == u.host);
        if exists {
            tracing::info!("UniFi source '{}' already present — skipped", u.host);
        } else {
            db::insert_unifi_source(pool, "UniFi", &u.host, &u.api_key, true).await?;
            tracing::info!("imported UniFi source '{}'", u.host);
        }
    }

    for p in cfg.proxmox {
        // proxmox_sources.name is unique — ON CONFLICT keeps re-runs a no-op.
        let res = sqlx::query(
            "INSERT INTO proxmox_sources (name, host, token_id, token_secret, enabled) \
             VALUES ($1, $2, $3, $4, true) ON CONFLICT (name) DO NOTHING",
        )
        .bind(&p.name)
        .bind(&p.host)
        .bind(&p.token_id)
        .bind(&p.token_secret)
        .execute(pool)
        .await
        .with_context(|| format!("importing Proxmox source '{}'", p.name))?;
        if res.rows_affected() > 0 {
            tracing::info!("imported Proxmox source '{}'", p.name);
        } else {
            tracing::info!("Proxmox source '{}' already present — skipped", p.name);
        }
    }
    Ok(())
}

async fn import_history(pool: &PgPool) -> anyhow::Result<()> {
    let path = "data/history.json";
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("no history file at '{path}' ({e}) — skipping history import");
            return Ok(());
        }
    };
    let history: LegacyHistory =
        serde_json::from_str(&text).with_context(|| format!("parsing legacy history '{path}'"))?;

    let total = history.samples.len();
    for s in &history.samples {
        // insert_sample is ON CONFLICT (ts) DO NOTHING — re-import is harmless.
        db::insert_sample(pool, s).await?;
    }
    tracing::info!("imported {total} history sample(s) from '{path}'");
    Ok(())
}
