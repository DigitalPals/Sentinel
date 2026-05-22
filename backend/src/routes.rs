//! HTTP surface: the JSON API plus the bundled single-page frontend.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::config::{AlertThresholds, ProxmoxConfig, RuntimeConfig, UiPrefs, UnifiConfig};
use crate::db;
use crate::engine::{patch_alerts, AppState};
use crate::proxmox::ProxmoxClient;
use crate::unifi::UnifiClient;

pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/api/snapshot", get(snapshot))
        .route("/api/health", get(health))
        .route("/api/alerts/action", post(alert_action))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/sources", get(get_sources))
        .route("/api/sources/test", post(test_source))
        .route("/api/sources/unifi", post(create_unifi))
        .route(
            "/api/sources/unifi/:id",
            put(update_unifi).delete(delete_unifi),
        )
        .route("/api/sources/proxmox", post(create_proxmox))
        .route(
            "/api/sources/proxmox/:id",
            put(update_proxmox).delete(delete_proxmox),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    match resolve_frontend_dir() {
        Some(dir) => {
            tracing::info!("serving frontend from {dir}");
            let index = format!("{dir}/index.html");
            api.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)))
        }
        None => {
            tracing::warn!("no built frontend found — API only (run `bun run build` in frontend/)");
            api.fallback(get(no_frontend))
        }
    }
}

// ── Error handling ──────────────────────────────────────────────────────────

/// A JSON API error, rendered as `{ "error": "..." }`.
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

fn bad_request(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

fn not_found(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, msg.into())
}

/// Reload the in-memory configuration so a settings/source change is visible
/// immediately, without waiting for the next poll cycle.
async fn refresh_config(state: &AppState) -> Result<(), ApiError> {
    let cfg = RuntimeConfig::load(&state.pool).await?;
    *state.config.write().unwrap() = Arc::new(cfg);
    Ok(())
}

// ── Monitoring snapshot ─────────────────────────────────────────────────────

/// The full monitoring snapshot consumed by the frontend.
async fn snapshot(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.current())
}

/// Source connectivity summary.
async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let s = state.current();
    Json(json!({
        "generatedAt": s.generated_at,
        "pollIntervalSec": s.poll_interval_sec,
        "sources": s.sources,
    }))
}

#[derive(Deserialize)]
struct ActionReq {
    id: String,
    action: String,
}

/// Apply an acknowledge / resolve / reopen action to a tracked alert.
async fn alert_action(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ActionReq>,
) -> impl IntoResponse {
    let applied = state.alerts.write().unwrap().apply(&req.id, &req.action);
    if applied {
        patch_alerts(&state);
        // Persist the workflow change so it survives a restart.
        let rows = state.alerts.read().unwrap().rows();
        if let Err(e) = db::save_alert_state(&state.pool, &rows).await {
            tracing::warn!("could not persist alert state: {e:#}");
        }
        (StatusCode::OK, "ok")
    } else {
        (StatusCode::NOT_FOUND, "unknown alert")
    }
}

// ── Settings ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsResponse {
    poll_interval_sec: u64,
    bind: String,
    http_timeout_sec: u64,
    history_max_samples: usize,
    history_retention_days: i64,
    frontend_poll_ms: u64,
    onboarding_done: bool,
    thresholds: AlertThresholds,
    ui: UiPrefs,
}

fn settings_response(c: &RuntimeConfig) -> SettingsResponse {
    SettingsResponse {
        poll_interval_sec: c.poll_interval_sec,
        bind: c.bind.clone(),
        http_timeout_sec: c.http_timeout_sec,
        history_max_samples: c.history_max_samples,
        history_retention_days: c.history_retention_days,
        frontend_poll_ms: c.frontend_poll_ms,
        onboarding_done: c.onboarding_done,
        thresholds: c.thresholds.clone(),
        ui: c.ui_prefs.clone(),
    }
}

/// Current tuning settings and UI preferences (no secrets).
async fn get_settings(State(state): State<Arc<AppState>>) -> ApiResult<SettingsResponse> {
    Ok(Json(settings_response(&state.config())))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsUpdate {
    poll_interval_sec: Option<u64>,
    bind: Option<String>,
    http_timeout_sec: Option<u64>,
    history_max_samples: Option<u64>,
    history_retention_days: Option<i64>,
    frontend_poll_ms: Option<u64>,
    onboarding_done: Option<bool>,
    thresholds: Option<AlertThresholds>,
    ui: Option<UiPrefs>,
}

/// Update one or more settings. Changes take effect on the next poll; `bind`
/// only applies after a restart.
async fn put_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SettingsUpdate>,
) -> ApiResult<SettingsResponse> {
    let pool = &state.pool;
    if let Some(v) = req.poll_interval_sec {
        db::set_setting(pool, "poll_interval_sec", &json!(v.max(5))).await?;
    }
    if let Some(v) = req.bind {
        db::set_setting(pool, "bind", &json!(v)).await?;
    }
    if let Some(v) = req.http_timeout_sec {
        db::set_setting(pool, "http_timeout_sec", &json!(v.max(1))).await?;
    }
    if let Some(v) = req.history_max_samples {
        db::set_setting(pool, "history_max_samples", &json!(v.max(1))).await?;
    }
    if let Some(v) = req.history_retention_days {
        db::set_setting(pool, "history_retention_days", &json!(v)).await?;
    }
    if let Some(v) = req.frontend_poll_ms {
        db::set_setting(pool, "frontend_poll_ms", &json!(v.max(500))).await?;
    }
    if let Some(v) = req.onboarding_done {
        db::set_setting(pool, "onboarding_done", &json!(v)).await?;
    }
    if let Some(v) = req.thresholds {
        let value = serde_json::to_value(v).map_err(|e| anyhow::anyhow!(e))?;
        db::set_setting(pool, "alert_thresholds", &value).await?;
    }
    if let Some(v) = req.ui {
        let value = serde_json::to_value(v).map_err(|e| anyhow::anyhow!(e))?;
        db::set_setting(pool, "ui_prefs", &value).await?;
    }
    refresh_config(&state).await?;
    Ok(Json(settings_response(&state.config())))
}

// ── Sources ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnifiSourceOut {
    id: i64,
    name: String,
    host: String,
    /// Whether an API key is stored — the key itself is never sent to clients.
    has_secret: bool,
    enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProxmoxSourceOut {
    id: i64,
    name: String,
    host: String,
    token_id: String,
    /// Whether a token secret is stored — the secret itself is never sent.
    has_secret: bool,
    enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourcesResponse {
    unifi: Vec<UnifiSourceOut>,
    proxmox: Vec<ProxmoxSourceOut>,
}

/// List all configured sources, with secrets masked.
async fn get_sources(State(state): State<Arc<AppState>>) -> ApiResult<SourcesResponse> {
    let unifi = db::get_unifi_sources(&state.pool)
        .await?
        .into_iter()
        .map(|r| UnifiSourceOut {
            id: r.id,
            name: r.name,
            host: r.host,
            has_secret: !r.api_key.is_empty(),
            enabled: r.enabled,
        })
        .collect();
    let proxmox = db::get_proxmox_sources(&state.pool)
        .await?
        .into_iter()
        .map(|r| ProxmoxSourceOut {
            id: r.id,
            name: r.name,
            host: r.host,
            token_id: r.token_id,
            has_secret: !r.token_secret.is_empty(),
            enabled: r.enabled,
        })
        .collect();
    Ok(Json(SourcesResponse { unifi, proxmox }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnifiSourceIn {
    name: Option<String>,
    host: String,
    /// Omitted or empty on update keeps the stored key.
    api_key: Option<String>,
    enabled: Option<bool>,
}

async fn create_unifi(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UnifiSourceIn>,
) -> ApiResult<serde_json::Value> {
    let host = req.host.trim();
    if host.is_empty() {
        return Err(bad_request("host is required"));
    }
    let api_key = req.api_key.unwrap_or_default();
    if api_key.trim().is_empty() {
        return Err(bad_request("API key is required"));
    }
    let name = req.name.unwrap_or_else(|| "UniFi".to_string());
    let id = db::insert_unifi_source(
        &state.pool,
        name.trim(),
        host,
        api_key.trim(),
        req.enabled.unwrap_or(true),
    )
    .await?;
    refresh_config(&state).await?;
    Ok(Json(json!({ "id": id })))
}

async fn update_unifi(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<UnifiSourceIn>,
) -> ApiResult<serde_json::Value> {
    let existing = db::get_unifi_source(&state.pool, id)
        .await?
        .ok_or_else(|| not_found("unknown UniFi source"))?;
    let host = req.host.trim();
    if host.is_empty() {
        return Err(bad_request("host is required"));
    }
    let api_key = match req.api_key {
        Some(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ => existing.api_key,
    };
    let name = req.name.unwrap_or(existing.name);
    let enabled = req.enabled.unwrap_or(existing.enabled);
    db::update_unifi_source(&state.pool, id, name.trim(), host, &api_key, enabled).await?;
    refresh_config(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_unifi(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<serde_json::Value> {
    if !db::delete_unifi_source(&state.pool, id).await? {
        return Err(not_found("unknown UniFi source"));
    }
    refresh_config(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxmoxSourceIn {
    name: String,
    host: String,
    token_id: String,
    /// Omitted or empty on update keeps the stored secret.
    token_secret: Option<String>,
    enabled: Option<bool>,
}

async fn create_proxmox(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ProxmoxSourceIn>,
) -> ApiResult<serde_json::Value> {
    let name = req.name.trim();
    let host = req.host.trim();
    let token_id = req.token_id.trim();
    if name.is_empty() || host.is_empty() || token_id.is_empty() {
        return Err(bad_request("name, host and token ID are required"));
    }
    let token_secret = req.token_secret.unwrap_or_default();
    if token_secret.trim().is_empty() {
        return Err(bad_request("token secret is required"));
    }
    let taken = db::get_proxmox_sources(&state.pool)
        .await?
        .iter()
        .any(|r| r.name == name);
    if taken {
        return Err(bad_request(format!("a Proxmox source named '{name}' already exists")));
    }
    let id = db::insert_proxmox_source(
        &state.pool,
        name,
        host,
        token_id,
        token_secret.trim(),
        req.enabled.unwrap_or(true),
    )
    .await?;
    refresh_config(&state).await?;
    Ok(Json(json!({ "id": id })))
}

async fn update_proxmox(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<ProxmoxSourceIn>,
) -> ApiResult<serde_json::Value> {
    let existing = db::get_proxmox_source(&state.pool, id)
        .await?
        .ok_or_else(|| not_found("unknown Proxmox source"))?;
    let name = req.name.trim();
    let host = req.host.trim();
    let token_id = req.token_id.trim();
    if name.is_empty() || host.is_empty() || token_id.is_empty() {
        return Err(bad_request("name, host and token ID are required"));
    }
    let token_secret = match req.token_secret {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => existing.token_secret,
    };
    db::update_proxmox_source(
        &state.pool,
        id,
        name,
        host,
        token_id,
        &token_secret,
        req.enabled.unwrap_or(existing.enabled),
    )
    .await?;
    refresh_config(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_proxmox(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<serde_json::Value> {
    if !db::delete_proxmox_source(&state.pool, id).await? {
        return Err(not_found("unknown Proxmox source"));
    }
    refresh_config(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestRequest {
    kind: String,
    /// Optional id of an existing source — used to fill in a secret the caller
    /// left blank (so an existing source can be tested without re-typing it).
    id: Option<i64>,
    host: String,
    api_key: Option<String>,
    token_id: Option<String>,
    token_secret: Option<String>,
}

#[derive(Serialize)]
struct TestResult {
    ok: bool,
    detail: String,
}

/// Probe a source's connectivity with the supplied (or stored) credentials.
async fn test_source(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TestRequest>,
) -> ApiResult<TestResult> {
    let timeout = state.config().http_timeout_sec;
    let host = req.host.trim().to_string();
    if host.is_empty() {
        return Err(bad_request("host is required"));
    }
    match req.kind.as_str() {
        "unifi" => {
            let api_key = match req.api_key {
                Some(k) if !k.trim().is_empty() => k.trim().to_string(),
                _ => match req.id {
                    Some(id) => {
                        db::get_unifi_source(&state.pool, id)
                            .await?
                            .ok_or_else(|| not_found("unknown UniFi source"))?
                            .api_key
                    }
                    None => return Err(bad_request("API key is required")),
                },
            };
            let client = UnifiClient::new(&UnifiConfig { host, api_key }, timeout)?;
            Ok(Json(match client.collect().await {
                Ok(d) => TestResult {
                    ok: true,
                    detail: format!("UniFi Network {} · {} device(s)", d.app_version, d.devices.len()),
                },
                Err(e) => TestResult {
                    ok: false,
                    detail: format!("{e:#}"),
                },
            }))
        }
        "proxmox" => {
            let stored = match req.id {
                Some(id) => db::get_proxmox_source(&state.pool, id).await?,
                None => None,
            };
            let token_id = match req.token_id {
                Some(t) if !t.trim().is_empty() => t.trim().to_string(),
                _ => stored
                    .as_ref()
                    .map(|s| s.token_id.clone())
                    .ok_or_else(|| bad_request("token ID is required"))?,
            };
            let token_secret = match req.token_secret {
                Some(s) if !s.trim().is_empty() => s.trim().to_string(),
                _ => stored
                    .as_ref()
                    .map(|s| s.token_secret.clone())
                    .ok_or_else(|| bad_request("token secret is required"))?,
            };
            let cfg = ProxmoxConfig {
                name: "test".to_string(),
                host,
                token_id,
                token_secret,
            };
            let client = ProxmoxClient::new(&cfg, timeout)?;
            Ok(Json(match client.collect().await {
                Ok(d) => TestResult {
                    ok: true,
                    detail: format!("Proxmox VE {} reachable", d.release),
                },
                Err(e) => TestResult {
                    ok: false,
                    detail: format!("{e:#}"),
                },
            }))
        }
        other => Err(bad_request(format!("unknown source kind '{other}'"))),
    }
}

// ── Frontend ────────────────────────────────────────────────────────────────

async fn no_frontend() -> impl IntoResponse {
    Html(
        "<!doctype html><meta charset=utf-8><title>Cybex Sentinel</title>\
         <body style=\"font-family:system-ui;background:#0a0c11;color:#e7ecf3;padding:48px\">\
         <h1>Cybex Sentinel — API is running</h1>\
         <p>The backend is live, but no built frontend was found.</p>\
         <p>Build it with <code>cd frontend &amp;&amp; bun install &amp;&amp; bun run build</code>, \
         then reload.</p>\
         <p>API: <a style=\"color:#7adfff\" href=\"/api/snapshot\">/api/snapshot</a></p>",
    )
}

/// Locate the built frontend, checking the usual relative locations.
fn resolve_frontend_dir() -> Option<String> {
    if let Ok(custom) = std::env::var("SENTINEL_FRONTEND") {
        if std::path::Path::new(&custom).join("index.html").exists() {
            return Some(custom);
        }
    }
    for candidate in ["../frontend/dist", "frontend/dist", "./dist", "dist"] {
        if std::path::Path::new(candidate).join("index.html").exists() {
            return Some(candidate.to_string());
        }
    }
    None
}
