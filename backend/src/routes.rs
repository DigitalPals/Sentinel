//! HTTP surface: the JSON API plus the bundled single-page frontend.

use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::auth;
use crate::config::{
    AlertThresholds, ProxmoxConfig, RuntimeConfig, UiPrefs, UnifiConfig, UnraidConfig,
};
use crate::db;
use crate::engine::{patch_alerts, AppState};
use crate::network_scanner::{self, settings_to_value, NetworkScanPort, NetworkScannerSettings};
use crate::notify::{
    self, NotificationChannel, NotificationSettingsPublic, NotificationSettingsUpdate,
};
use crate::proxmox::ProxmoxClient;
use crate::unifi::{DeviceBundle, LegacyClient, UniClient, UnifiClient};
use crate::unraid::UnraidClient;

pub fn router(state: Arc<AppState>) -> Router {
    // Public auth endpoints — reachable without a session, so the login page
    // can load and the first user can be created.
    let public = Router::new()
        .route("/api/auth/status", get(auth::auth_status))
        .route("/api/auth/setup", post(auth::auth_setup))
        .route("/api/auth/login", post(auth::auth_login))
        .route("/api/auth/logout", post(auth::auth_logout))
        .with_state(state.clone());

    // Everything else requires a valid login session.
    let protected = Router::new()
        .route("/api/snapshot", get(snapshot))
        .route("/api/health", get(health))
        .route("/api/alerts/action", post(alert_action))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/network-scanner", get(get_network_scanner))
        .route("/api/network-scanner/hosts/:target", get(get_network_host))
        .route(
            "/api/network-scanner/hosts/:target/unifi",
            get(get_network_host_unifi),
        )
        .route(
            "/api/network-scanner/hosts/:target/port-scan",
            post(start_host_port_scan),
        )
        .route("/api/network-scanner/scan", post(start_network_scan))
        .route(
            "/api/network-scanner/jobs/:id/cancel",
            post(cancel_network_scan),
        )
        .route("/api/notifications/test", post(test_notification))
        .route("/api/push/status", get(push_status))
        .route(
            "/api/push/subscriptions",
            post(save_push_subscription).delete(delete_push_subscription),
        )
        .route("/api/sources", get(get_sources))
        .route("/api/sources/test", post(test_source))
        .route("/api/sources/unifi", post(create_unifi))
        .route(
            "/api/sources/unifi/:id",
            put(update_unifi).delete(delete_unifi),
        )
        .route("/api/sources/unraid", post(create_unraid))
        .route(
            "/api/sources/unraid/:id",
            put(update_unraid).delete(delete_unraid),
        )
        .route("/api/sources/proxmox", post(create_proxmox))
        .route(
            "/api/sources/proxmox/:id",
            put(update_proxmox).delete(delete_proxmox),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_session,
        ))
        .with_state(state);

    // No CORS layer: the backend serves its own SPA (same-origin), and the dev
    // proxy makes requests same-origin too. A wildcard origin would, in any
    // case, be rejected by browsers on the credentialed (cookie) requests.
    let api = public.merge(protected).layer(TraceLayer::new_for_http());

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
pub(crate) struct ApiError(pub(crate) StatusCode, pub(crate) String);

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

pub(crate) type ApiResult<T> = Result<Json<T>, ApiError>;

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
    db::configure_metric_retention(&state.pool, cfg.history_retention_days).await?;
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
    thresholds: AlertThresholds,
    ui: UiPrefs,
    notifications: NotificationSettingsPublic,
    network_scanner: NetworkScannerSettings,
}

fn settings_response(c: &RuntimeConfig) -> SettingsResponse {
    SettingsResponse {
        poll_interval_sec: c.poll_interval_sec,
        bind: c.bind.clone(),
        http_timeout_sec: c.http_timeout_sec,
        history_max_samples: c.history_max_samples,
        history_retention_days: c.history_retention_days,
        frontend_poll_ms: c.frontend_poll_ms,
        thresholds: c.thresholds.clone(),
        ui: c.ui_prefs.clone(),
        notifications: c.notifications.public(),
        network_scanner: c.network_scanner.clone(),
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
    thresholds: Option<AlertThresholds>,
    ui: Option<UiPrefs>,
    notifications: Option<NotificationSettingsUpdate>,
    network_scanner: Option<NetworkScannerSettings>,
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
        db::set_setting(pool, "history_retention_days", &json!(v.max(1))).await?;
    }
    if let Some(v) = req.frontend_poll_ms {
        db::set_setting(pool, "frontend_poll_ms", &json!(v.max(500))).await?;
    }
    if let Some(v) = req.thresholds {
        let value = serde_json::to_value(v).map_err(|e| anyhow::anyhow!(e))?;
        db::set_setting(pool, "alert_thresholds", &value).await?;
    }
    if let Some(v) = req.ui {
        let value = serde_json::to_value(v).map_err(|e| anyhow::anyhow!(e))?;
        db::set_setting(pool, "ui_prefs", &value).await?;
    }
    if let Some(v) = req.notifications {
        let mut notifications = state.config().notifications.clone();
        notifications.apply_update(v);
        if notifications.push.enabled {
            notify::ensure_push_vapid(&mut notifications)
                .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
        }
        let value = serde_json::to_value(notifications).map_err(|e| anyhow::anyhow!(e))?;
        db::set_setting(pool, "notifications", &value).await?;
    }
    if let Some(v) = req.network_scanner {
        let value = settings_to_value(&v)
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
        db::set_setting(pool, network_scanner::SETTINGS_KEY, &value).await?;
    }
    refresh_config(&state).await?;
    Ok(Json(settings_response(&state.config())))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotificationTestReq {
    channel: NotificationChannel,
    notifications: Option<NotificationSettingsUpdate>,
}

/// Send a test notification using the saved configuration plus any unsaved
/// form edits supplied by the client. The candidate config is not persisted.
async fn test_notification(
    State(state): State<Arc<AppState>>,
    Json(req): Json<NotificationTestReq>,
) -> ApiResult<serde_json::Value> {
    let mut notifications = state.config().notifications.clone();
    if let Some(update) = req.notifications {
        notifications.apply_update(update);
    }
    notify::send_test_notification(&state.pool, &notifications, &req.channel)
        .await
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
    Ok(Json(
        json!({ "ok": true, "detail": "Test notification sent." }),
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PushStatusResponse {
    enabled: bool,
    configured: bool,
    public_key: String,
    subscription_count: i64,
}

async fn ensure_push_config(
    state: &AppState,
) -> Result<notify::NotificationSettingsPublic, ApiError> {
    let mut notifications = state.config().notifications.clone();
    if notifications.push.vapid_private_key.trim().is_empty() {
        notify::ensure_push_vapid(&mut notifications)
            .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?;
        let value = serde_json::to_value(&notifications).map_err(|e| anyhow::anyhow!(e))?;
        db::set_setting(&state.pool, "notifications", &value).await?;
        refresh_config(state).await?;
        notifications = state.config().notifications.clone();
    }
    Ok(notifications.public())
}

async fn push_status(State(state): State<Arc<AppState>>) -> ApiResult<PushStatusResponse> {
    let public = ensure_push_config(&state).await?;
    let public_key = public.push.public_key.ok_or_else(|| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid browser push VAPID key".to_string(),
        )
    })?;
    let subscription_count = db::count_push_subscriptions(&state.pool).await?;
    Ok(Json(PushStatusResponse {
        enabled: public.push.enabled,
        configured: public.push.configured,
        public_key,
        subscription_count,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushSubscriptionIn {
    endpoint: String,
    keys: PushSubscriptionKeysIn,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushSubscriptionKeysIn {
    p256dh: String,
    auth: String,
}

async fn save_push_subscription(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<PushSubscriptionIn>,
) -> ApiResult<serde_json::Value> {
    if req.endpoint.trim().is_empty()
        || req.keys.p256dh.trim().is_empty()
        || req.keys.auth.trim().is_empty()
    {
        return Err(bad_request("invalid browser push subscription"));
    }
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    db::upsert_push_subscription(
        &state.pool,
        req.endpoint.trim(),
        req.keys.p256dh.trim(),
        req.keys.auth.trim(),
        user_agent.as_deref(),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeletePushSubscriptionReq {
    endpoint: String,
}

async fn delete_push_subscription(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeletePushSubscriptionReq>,
) -> ApiResult<serde_json::Value> {
    if req.endpoint.trim().is_empty() {
        return Err(bad_request("invalid browser push subscription"));
    }
    let deleted = db::delete_push_subscription(&state.pool, req.endpoint.trim()).await?;
    Ok(Json(json!({ "ok": true, "deleted": deleted })))
}

// ── Network scanner ─────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkScanJobOut {
    id: i64,
    status: String,
    trigger: String,
    summary: Option<serde_json::Value>,
    error: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<db::NetworkScanJobRow> for NetworkScanJobOut {
    fn from(r: db::NetworkScanJobRow) -> Self {
        Self {
            id: r.id,
            status: r.status,
            trigger: r.trigger,
            summary: r.summary,
            error: r.error,
            created_at: r.created_at,
            started_at: r.started_at,
            finished_at: r.finished_at,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkScanDeviceOut {
    id: i64,
    job_id: Option<i64>,
    ip: String,
    hostname: Option<String>,
    mac: Option<String>,
    vendor: Option<String>,
    status: String,
    discovery_method: String,
    latency_ms: Option<f64>,
    ports: Vec<NetworkScanPort>,
    os_guess: Option<String>,
    first_seen: Option<chrono::DateTime<chrono::Utc>>,
    last_seen: chrono::DateTime<chrono::Utc>,
}

impl From<db::NetworkScanDeviceRow> for NetworkScanDeviceOut {
    fn from(r: db::NetworkScanDeviceRow) -> Self {
        Self {
            id: r.id,
            job_id: r.job_id,
            ip: r.ip,
            hostname: r.hostname,
            mac: r.mac,
            vendor: r.vendor,
            status: r.status,
            discovery_method: r.discovery_method,
            latency_ms: r.latency_ms,
            ports: serde_json::from_value(r.ports).unwrap_or_default(),
            os_guess: r.os_guess,
            first_seen: r.first_seen,
            last_seen: r.last_seen,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkHostTrafficOut {
    tx_mbps: Option<f64>,
    rx_mbps: Option<f64>,
    tx_bytes: Option<u64>,
    rx_bytes: Option<u64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NetworkHostUnifiDeviceOut {
    id: String,
    name: String,
    kind: String,
    model: String,
    ip: String,
    mac: String,
    state: String,
    site: String,
    tx_mbps: f64,
    rx_mbps: f64,
    clients: u32,
    cpu: u32,
    mem: u32,
    firmware: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkHostUnifiClientOut {
    id: Option<String>,
    name: Option<String>,
    kind: Option<String>,
    ip: Option<String>,
    mac: Option<String>,
    network_name: Option<String>,
    ssid: Option<String>,
    connected_at: Option<String>,
    last_seen_at: Option<String>,
    signal: Option<i64>,
    channel: Option<u32>,
    vlan_id: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkHostConnectionOut {
    connection_type: String,
    uplink_device: Option<NetworkHostUnifiDeviceOut>,
    uplink_device_id: Option<String>,
    uplink_device_name: Option<String>,
    port_idx: Option<u32>,
    port_name: Option<String>,
    port_state: Option<String>,
    port_speed_mbps: Option<u32>,
    poe: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkHostUnifiOut {
    configured: bool,
    site: Option<String>,
    app_version: Option<String>,
    matched_by: Option<String>,
    client: Option<NetworkHostUnifiClientOut>,
    device: Option<NetworkHostUnifiDeviceOut>,
    connection: Option<NetworkHostConnectionOut>,
    traffic: Option<NetworkHostTrafficOut>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkScannerResponse {
    settings: NetworkScannerSettings,
    active_job: Option<NetworkScanJobOut>,
    latest_job: Option<NetworkScanJobOut>,
    jobs: Vec<NetworkScanJobOut>,
    devices: Vec<NetworkScanDeviceOut>,
    inventory: Vec<NetworkScanDeviceOut>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkHostDetailResponse {
    target: String,
    settings: NetworkScannerSettings,
    active_job: Option<NetworkScanJobOut>,
    host: Option<NetworkScanDeviceOut>,
    observations: Vec<NetworkScanDeviceOut>,
    unifi: Option<NetworkHostUnifiOut>,
}

async fn get_network_scanner(
    State(state): State<Arc<AppState>>,
) -> ApiResult<NetworkScannerResponse> {
    let active = db::active_network_scan_job(&state.pool).await?;
    let latest = db::latest_completed_network_scan_job(&state.pool).await?;
    let devices = match latest.as_ref() {
        Some(job) => db::network_scan_devices(&state.pool, job.id).await?,
        None => Vec::new(),
    };
    let jobs = db::recent_network_scan_jobs(&state.pool, 20).await?;
    let inventory = db::network_scan_inventory(&state.pool).await?;
    Ok(Json(NetworkScannerResponse {
        settings: state.config().network_scanner.clone(),
        active_job: active.map(Into::into),
        latest_job: latest.map(Into::into),
        jobs: jobs.into_iter().map(Into::into).collect(),
        devices: devices.into_iter().map(Into::into).collect(),
        inventory: inventory.into_iter().map(Into::into).collect(),
    }))
}

async fn get_network_host(
    State(state): State<Arc<AppState>>,
    Path(target): Path<String>,
) -> ApiResult<NetworkHostDetailResponse> {
    let target = target.trim().to_string();
    if target.is_empty() {
        return Err(bad_request("host target is required"));
    }

    let inventory = db::network_scan_inventory_host(&state.pool, &target).await?;
    let observations = db::network_scan_host_observations(&state.pool, &target, 30).await?;
    let host = inventory.clone().or_else(|| observations.first().cloned());

    if host.is_none() {
        return Err(not_found("unknown network host"));
    }

    let active = db::active_network_scan_job(&state.pool).await?;
    Ok(Json(NetworkHostDetailResponse {
        target,
        settings: state.config().network_scanner.clone(),
        active_job: active.map(Into::into),
        host: host.map(Into::into),
        observations: observations.into_iter().map(Into::into).collect(),
        unifi: None,
    }))
}

async fn get_network_host_unifi(
    State(state): State<Arc<AppState>>,
    Path(target): Path<String>,
) -> ApiResult<NetworkHostUnifiOut> {
    let target = target.trim().to_string();
    if target.is_empty() {
        return Err(bad_request("host target is required"));
    }
    let inventory = db::network_scan_inventory_host(&state.pool, &target).await?;
    let observations = db::network_scan_host_observations(&state.pool, &target, 1).await?;
    let host = inventory.as_ref().or_else(|| observations.first());
    Ok(Json(build_unifi_host_detail(&state, &target, host).await))
}

async fn build_unifi_host_detail(
    state: &AppState,
    target: &str,
    host: Option<&db::NetworkScanDeviceRow>,
) -> NetworkHostUnifiOut {
    let cfg = state.config();
    let Some(unifi_cfg) = cfg.unifi.clone() else {
        return NetworkHostUnifiOut {
            configured: false,
            site: None,
            app_version: None,
            matched_by: None,
            client: None,
            device: None,
            connection: None,
            traffic: None,
            error: None,
        };
    };
    let timeout = cfg.http_timeout_sec;
    drop(cfg);

    let client = match UnifiClient::new(&unifi_cfg, timeout) {
        Ok(c) => c,
        Err(e) => {
            return NetworkHostUnifiOut {
                configured: true,
                site: None,
                app_version: None,
                matched_by: None,
                client: None,
                device: None,
                connection: None,
                traffic: None,
                error: Some(format!("{e:#}")),
            }
        }
    };
    let data = match client.collect().await {
        Ok(d) => d,
        Err(e) => {
            return NetworkHostUnifiOut {
                configured: true,
                site: None,
                app_version: None,
                matched_by: None,
                client: None,
                device: None,
                connection: None,
                traffic: None,
                error: Some(format!("{e:#}")),
            }
        }
    };

    let host_ip = host.map(|h| h.ip.as_str()).unwrap_or(target);
    let host_mac = host.and_then(|h| h.mac.as_deref());

    let mut devices_by_id: HashMap<String, &DeviceBundle> = HashMap::new();
    for device in &data.devices {
        devices_by_id.insert(device.list.id.clone(), device);
    }

    let mut clients_per_device: HashMap<String, u32> = HashMap::new();
    for c in &data.clients {
        if let Some(id) = client_uplink_device_id(c) {
            *clients_per_device.entry(id).or_insert(0) += 1;
        }
    }

    let legacy_match = match client.legacy_clients(&data.site_reference).await {
        Ok(clients) => clients
            .into_iter()
            .find(|c| unifi_legacy_client_matches(c, target, host_ip, host_mac)),
        Err(e) => {
            tracing::debug!("could not load UniFi legacy client stats: {e:#}");
            None
        }
    };

    let matched_device = data
        .devices
        .iter()
        .find(|d| unifi_device_matches(d, target, host_ip, host_mac));

    let mut matched_client = data
        .clients
        .iter()
        .find(|c| unifi_client_matches(c, target, host_ip, host_mac))
        .cloned();

    if let Some(base) = matched_client.take() {
        matched_client = match client_client_id(&base) {
            Some(id) => match client.client_detail(&data.site_id, &id).await {
                Ok(detail) => Some(merge_unifi_client(base, detail)),
                Err(_) => Some(base),
            },
            None => Some(base),
        };
    }

    let matched_by = matched_device
        .and_then(|d| unifi_device_match_kind(d, target, host_ip, host_mac))
        .or_else(|| {
            matched_client
                .as_ref()
                .and_then(|c| unifi_client_match_kind(c, target, host_ip, host_mac))
        })
        .or_else(|| {
            legacy_match
                .as_ref()
                .and_then(|c| unifi_legacy_client_match_kind(c, target, host_ip, host_mac))
        });

    let device_out = matched_device.map(|d| {
        unifi_device_out(
            d,
            &data.site,
            clients_per_device.get(&d.list.id).copied().unwrap_or(0),
        )
    });
    let client_out = matched_client
        .as_ref()
        .map(|c| unifi_client_out(c, legacy_match.as_ref()))
        .or_else(|| legacy_match.as_ref().map(unifi_legacy_client_out));

    let connection = if let Some(device) = matched_device {
        unifi_device_connection(device, &devices_by_id, &clients_per_device, &data.site)
    } else if let Some(legacy) = legacy_match.as_ref() {
        unifi_legacy_client_connection(
            legacy,
            &data.devices,
            &clients_per_device,
            &data.site,
        )
    } else {
        matched_client
            .as_ref()
            .map(|c| unifi_client_connection(c, &devices_by_id, &clients_per_device, &data.site))
    };

    let traffic = matched_device
        .map(unifi_device_traffic)
        .or_else(|| legacy_match.as_ref().map(unifi_legacy_client_traffic))
        .or_else(|| matched_client.as_ref().map(unifi_client_traffic));

    NetworkHostUnifiOut {
        configured: true,
        site: Some(data.site.clone()),
        app_version: Some(data.app_version.clone()),
        matched_by,
        client: client_out,
        device: device_out,
        connection,
        traffic,
        error: None,
    }
}

fn unifi_device_out(bundle: &DeviceBundle, site: &str, clients: u32) -> NetworkHostUnifiDeviceOut {
    let tx_rate = bundle
        .stats
        .uplink
        .as_ref()
        .and_then(|u| u.tx_rate_bps)
        .unwrap_or(0);
    let rx_rate = bundle
        .stats
        .uplink
        .as_ref()
        .and_then(|u| u.rx_rate_bps)
        .unwrap_or(0);
    NetworkHostUnifiDeviceOut {
        id: bundle.list.id.clone(),
        name: bundle
            .list
            .name
            .clone()
            .unwrap_or_else(|| bundle.list.id.clone()),
        kind: unifi_device_kind(bundle).to_string(),
        model: bundle.list.model.clone().unwrap_or_default(),
        ip: bundle.list.ip_address.clone().unwrap_or_default(),
        mac: bundle.list.mac_address.clone().unwrap_or_default(),
        state: bundle.list.state.clone().unwrap_or_default(),
        site: site.to_string(),
        tx_mbps: rate_to_mbps(tx_rate),
        rx_mbps: rate_to_mbps(rx_rate),
        clients,
        cpu: bundle.stats.cpu_utilization_pct.unwrap_or(0.0).round() as u32,
        mem: bundle.stats.memory_utilization_pct.unwrap_or(0.0).round() as u32,
        firmware: bundle.list.firmware_version.clone().unwrap_or_default(),
    }
}

fn unifi_client_out(
    client: &UniClient,
    legacy: Option<&LegacyClient>,
) -> NetworkHostUnifiClientOut {
    NetworkHostUnifiClientOut {
        id: client_client_id(client),
        name: client_display_name(client).or_else(|| legacy.and_then(legacy_display_name)),
        kind: client.kind.clone(),
        ip: client_ip(client),
        mac: client_mac(client),
        network_name: client
            .network_name
            .clone()
            .or_else(|| extra_string(client, &["network", "networkName"]))
            .or_else(|| legacy.and_then(legacy_network_name)),
        ssid: client
            .ssid
            .clone()
            .or_else(|| extra_string(client, &["ssid", "essid"])),
        connected_at: client
            .connected_at
            .clone()
            .or_else(|| extra_string(client, &["connectedAt", "firstSeenAt"]))
            .or_else(|| legacy.and_then(|c| c.first_seen).map(epoch_to_rfc3339)),
        last_seen_at: client
            .last_seen_at
            .clone()
            .or_else(|| extra_string(client, &["lastSeenAt", "lastSeen"]))
            .or_else(|| legacy.and_then(|c| c.last_seen).map(epoch_to_rfc3339)),
        signal: client
            .signal
            .or_else(|| extra_i64(client, &["signal", "rssi"]))
            .or_else(|| legacy.and_then(|c| c.signal)),
        channel: client
            .channel
            .or_else(|| extra_u32(client, &["channel"]))
            .or_else(|| legacy.and_then(|c| c.channel)),
        vlan_id: client
            .vlan_id
            .or_else(|| extra_u32(client, &["vlanId", "vlan"]))
            .or_else(|| legacy.and_then(legacy_vlan)),
    }
}

fn unifi_legacy_client_out(client: &LegacyClient) -> NetworkHostUnifiClientOut {
    NetworkHostUnifiClientOut {
        id: client.id.clone().or_else(|| client.mac.clone()),
        name: legacy_display_name(client),
        kind: Some(if client.is_wired.unwrap_or(false) {
            "WIRED".to_string()
        } else {
            "WIRELESS".to_string()
        }),
        ip: legacy_ip(client),
        mac: client.mac.clone(),
        network_name: legacy_network_name(client),
        ssid: None,
        connected_at: client.first_seen.map(epoch_to_rfc3339),
        last_seen_at: client.last_seen.map(epoch_to_rfc3339),
        signal: client.signal,
        channel: client.channel,
        vlan_id: legacy_vlan(client),
    }
}

fn unifi_client_connection(
    client: &UniClient,
    devices_by_id: &HashMap<String, &DeviceBundle>,
    clients_per_device: &HashMap<String, u32>,
    site: &str,
) -> NetworkHostConnectionOut {
    let uplink_id = client_uplink_device_id(client);
    let uplink = uplink_id
        .as_ref()
        .and_then(|id| devices_by_id.get(id).copied());
    let port_idx = client_uplink_port(client);
    let port = uplink.and_then(|d| unifi_port(d, port_idx));
    NetworkHostConnectionOut {
        connection_type: client
            .kind
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
            .to_ascii_lowercase(),
        uplink_device: uplink.map(|d| {
            unifi_device_out(
                d,
                site,
                clients_per_device.get(&d.list.id).copied().unwrap_or(0),
            )
        }),
        uplink_device_id: uplink_id.clone(),
        uplink_device_name: client
            .uplink_device_name
            .clone()
            .or_else(|| extra_string(client, &["uplinkDeviceName"])),
        port_idx,
        port_name: client
            .uplink_port_name
            .clone()
            .or_else(|| extra_string(client, &["uplinkPortName", "switchPortName"])),
        port_state: port.and_then(|p| p.state.clone()),
        port_speed_mbps: port.and_then(|p| p.speed_mbps),
        poe: port.and_then(|p| p.poe.as_ref().map(|poe| poe.enabled)),
    }
}

fn unifi_legacy_client_connection(
    client: &LegacyClient,
    devices: &[DeviceBundle],
    clients_per_device: &HashMap<String, u32>,
    site: &str,
) -> Option<NetworkHostConnectionOut> {
    let switch_mac = client.sw_mac.as_deref().or(client.last_uplink_mac.as_deref());
    let uplink = switch_mac.and_then(|mac| {
        devices.iter().find(|d| {
            d.list
                .mac_address
                .as_deref()
                .map(|device_mac| mac_eq(device_mac, mac))
                .unwrap_or(false)
        })
    });
    let port_idx = client.sw_port.or(client.last_uplink_remote_port);
    let port = uplink.and_then(|d| unifi_port(d, port_idx));
    Some(NetworkHostConnectionOut {
        connection_type: if client.is_wired.unwrap_or(false) {
            "wired".to_string()
        } else {
            "wireless".to_string()
        },
        uplink_device: uplink.map(|d| {
            unifi_device_out(
                d,
                site,
                clients_per_device.get(&d.list.id).copied().unwrap_or(0),
            )
        }),
        uplink_device_id: uplink.map(|d| d.list.id.clone()),
        uplink_device_name: client.last_uplink_name.clone(),
        port_idx,
        port_name: port_idx.map(|idx| format!("Port {idx}")),
        port_state: port.and_then(|p| p.state.clone()),
        port_speed_mbps: port
            .and_then(|p| p.speed_mbps)
            .or(client.wired_rate_mbps),
        poe: port.and_then(|p| p.poe.as_ref().map(|poe| poe.enabled)),
    })
}

fn unifi_device_connection(
    device: &DeviceBundle,
    devices_by_id: &HashMap<String, &DeviceBundle>,
    clients_per_device: &HashMap<String, u32>,
    site: &str,
) -> Option<NetworkHostConnectionOut> {
    let uplink = device.detail.uplink.as_ref()?;
    let uplink_id = uplink.device_id.clone();
    let uplink_device = uplink_id
        .as_ref()
        .and_then(|id| devices_by_id.get(id).copied());
    let port = uplink_device.and_then(|d| unifi_port(d, uplink.port_idx));
    Some(NetworkHostConnectionOut {
        connection_type: "device-uplink".to_string(),
        uplink_device: uplink_device.map(|d| {
            unifi_device_out(
                d,
                site,
                clients_per_device.get(&d.list.id).copied().unwrap_or(0),
            )
        }),
        uplink_device_id: uplink_id,
        uplink_device_name: uplink.device_name.clone(),
        port_idx: uplink.port_idx,
        port_name: uplink.port_name.clone(),
        port_state: port.and_then(|p| p.state.clone()),
        port_speed_mbps: port.and_then(|p| p.speed_mbps),
        poe: port.and_then(|p| p.poe.as_ref().map(|poe| poe.enabled)),
    })
}

fn unifi_device_traffic(bundle: &DeviceBundle) -> NetworkHostTrafficOut {
    let tx_rate = bundle.stats.uplink.as_ref().and_then(|u| u.tx_rate_bps);
    let rx_rate = bundle.stats.uplink.as_ref().and_then(|u| u.rx_rate_bps);
    NetworkHostTrafficOut {
        tx_mbps: tx_rate.map(rate_to_mbps),
        rx_mbps: rx_rate.map(rate_to_mbps),
        tx_bytes: None,
        rx_bytes: None,
    }
}

fn unifi_client_traffic(client: &UniClient) -> NetworkHostTrafficOut {
    let tx_rate = client
        .tx_rate_bps
        .or_else(|| extra_u64(client, &["txRateBps", "txRate", "tx_rate_bps"]));
    let rx_rate = client
        .rx_rate_bps
        .or_else(|| extra_u64(client, &["rxRateBps", "rxRate", "rx_rate_bps"]));
    NetworkHostTrafficOut {
        tx_mbps: tx_rate.map(rate_to_mbps),
        rx_mbps: rx_rate.map(rate_to_mbps),
        tx_bytes: client
            .tx_bytes
            .or_else(|| extra_u64(client, &["txBytes", "bytesOut"])),
        rx_bytes: client
            .rx_bytes
            .or_else(|| extra_u64(client, &["rxBytes", "bytesIn"])),
    }
}

fn unifi_legacy_client_traffic(client: &LegacyClient) -> NetworkHostTrafficOut {
    let tx_rate = client.wired_tx_bytes_rate.or(client.tx_bytes_rate);
    let rx_rate = client.wired_rx_bytes_rate.or(client.rx_bytes_rate);
    NetworkHostTrafficOut {
        tx_mbps: tx_rate.map(bytes_per_sec_to_mbps),
        rx_mbps: rx_rate.map(bytes_per_sec_to_mbps),
        tx_bytes: client.wired_tx_bytes.or(client.tx_bytes),
        rx_bytes: client.wired_rx_bytes.or(client.rx_bytes),
    }
}

fn unifi_device_matches(
    device: &DeviceBundle,
    target: &str,
    host_ip: &str,
    host_mac: Option<&str>,
) -> bool {
    device
        .list
        .ip_address
        .as_deref()
        .map(|ip| ip == target || ip == host_ip)
        .unwrap_or(false)
        || host_mac
            .and_then(|mac| {
                device
                    .list
                    .mac_address
                    .as_deref()
                    .map(|dmac| mac_eq(dmac, mac))
            })
            .unwrap_or(false)
        || device
            .list
            .mac_address
            .as_deref()
            .map(|mac| mac_eq(mac, target))
            .unwrap_or(false)
}

fn unifi_device_match_kind(
    device: &DeviceBundle,
    target: &str,
    host_ip: &str,
    host_mac: Option<&str>,
) -> Option<String> {
    if device
        .list
        .ip_address
        .as_deref()
        .map(|ip| ip == target || ip == host_ip)
        .unwrap_or(false)
    {
        return Some("unifi-device-ip".to_string());
    }
    if let Some(mac) = host_mac {
        if device
            .list
            .mac_address
            .as_deref()
            .map(|dmac| mac_eq(dmac, mac))
            .unwrap_or(false)
        {
            return Some("unifi-device-mac".to_string());
        }
    }
    if device
        .list
        .mac_address
        .as_deref()
        .map(|mac| mac_eq(mac, target))
        .unwrap_or(false)
    {
        return Some("unifi-device-mac".to_string());
    }
    None
}

fn unifi_client_matches(
    client: &UniClient,
    target: &str,
    host_ip: &str,
    host_mac: Option<&str>,
) -> bool {
    client_ip(client)
        .as_deref()
        .map(|ip| ip == target || ip == host_ip)
        .unwrap_or(false)
        || host_mac
            .map(|mac| {
                client_mac(client)
                    .as_deref()
                    .map(|cmac| mac_eq(cmac, mac))
                    .unwrap_or(false)
                    || client_client_id(client)
                        .as_deref()
                        .map(|id| mac_eq(id, mac))
                        .unwrap_or(false)
            })
            .unwrap_or(false)
        || client_mac(client)
            .as_deref()
            .map(|mac| mac_eq(mac, target))
            .unwrap_or(false)
        || client_client_id(client)
            .as_deref()
            .map(|id| mac_eq(id, target))
            .unwrap_or(false)
}

fn unifi_client_match_kind(
    client: &UniClient,
    target: &str,
    host_ip: &str,
    host_mac: Option<&str>,
) -> Option<String> {
    if client_ip(client)
        .as_deref()
        .map(|ip| ip == target || ip == host_ip)
        .unwrap_or(false)
    {
        return Some("unifi-client-ip".to_string());
    }
    if let Some(mac) = host_mac {
        if client_mac(client)
            .as_deref()
            .map(|cmac| mac_eq(cmac, mac))
            .unwrap_or(false)
            || client_client_id(client)
                .as_deref()
                .map(|id| mac_eq(id, mac))
                .unwrap_or(false)
        {
            return Some("unifi-client-mac".to_string());
        }
    }
    if client_mac(client)
        .as_deref()
        .map(|mac| mac_eq(mac, target))
        .unwrap_or(false)
        || client_client_id(client)
            .as_deref()
            .map(|id| mac_eq(id, target))
            .unwrap_or(false)
    {
        return Some("unifi-client-mac".to_string());
    }
    None
}

fn unifi_legacy_client_matches(
    client: &LegacyClient,
    target: &str,
    host_ip: &str,
    host_mac: Option<&str>,
) -> bool {
    legacy_ip(client)
        .as_deref()
        .map(|ip| ip == target || ip == host_ip)
        .unwrap_or(false)
        || client
            .mac
            .as_deref()
            .map(|mac| {
                mac_eq(mac, target) || host_mac.map(|host_mac| mac_eq(mac, host_mac)).unwrap_or(false)
            })
            .unwrap_or(false)
}

fn unifi_legacy_client_match_kind(
    client: &LegacyClient,
    target: &str,
    host_ip: &str,
    host_mac: Option<&str>,
) -> Option<String> {
    if legacy_ip(client)
        .as_deref()
        .map(|ip| ip == target || ip == host_ip)
        .unwrap_or(false)
    {
        return Some("unifi-client-ip".to_string());
    }
    if client
        .mac
        .as_deref()
        .map(|mac| {
            mac_eq(mac, target) || host_mac.map(|host_mac| mac_eq(mac, host_mac)).unwrap_or(false)
        })
        .unwrap_or(false)
    {
        return Some("unifi-client-mac".to_string());
    }
    None
}

fn unifi_device_kind(bundle: &DeviceBundle) -> &'static str {
    let model = bundle.list.model.as_deref().unwrap_or_default();
    let features = &bundle.list.features;
    let is_ap = features.iter().any(|f| f == "accessPoint");
    if model.contains("UCG")
        || model.contains("UDM")
        || model.contains("UXG")
        || model.contains("UDR")
    {
        "Gateway"
    } else if is_ap {
        "Access Point"
    } else if model.contains("UPS") {
        "UPS"
    } else {
        "Switch"
    }
}

fn unifi_port(device: &DeviceBundle, port_idx: Option<u32>) -> Option<&crate::unifi::Port> {
    let idx = port_idx?;
    device
        .detail
        .interfaces
        .as_ref()?
        .ports
        .iter()
        .find(|p| p.idx == idx)
}

fn merge_unifi_client(base: UniClient, detail: UniClient) -> UniClient {
    let mut extra = base.extra;
    extra.extend(detail.extra);
    UniClient {
        id: detail.id.or(base.id),
        kind: detail.kind.or(base.kind),
        mac_address: detail.mac_address.or(base.mac_address),
        ip_address: detail.ip_address.or(base.ip_address),
        name: detail.name.or(base.name),
        hostname: detail.hostname.or(base.hostname),
        display_name: detail.display_name.or(base.display_name),
        network_name: detail.network_name.or(base.network_name),
        ssid: detail.ssid.or(base.ssid),
        connected_at: detail.connected_at.or(base.connected_at),
        last_seen_at: detail.last_seen_at.or(base.last_seen_at),
        uplink_device_id: detail.uplink_device_id.or(base.uplink_device_id),
        uplink_device_name: detail.uplink_device_name.or(base.uplink_device_name),
        uplink_port: detail.uplink_port.or(base.uplink_port),
        uplink_port_name: detail.uplink_port_name.or(base.uplink_port_name),
        rx_rate_bps: detail.rx_rate_bps.or(base.rx_rate_bps),
        tx_rate_bps: detail.tx_rate_bps.or(base.tx_rate_bps),
        rx_bytes: detail.rx_bytes.or(base.rx_bytes),
        tx_bytes: detail.tx_bytes.or(base.tx_bytes),
        signal: detail.signal.or(base.signal),
        channel: detail.channel.or(base.channel),
        vlan_id: detail.vlan_id.or(base.vlan_id),
        extra,
    }
}

fn client_client_id(client: &UniClient) -> Option<String> {
    client
        .id
        .clone()
        .or_else(|| extra_string(client, &["id", "_id", "clientId"]))
        .or_else(|| client.mac_address.clone())
        .or_else(|| extra_string(client, &["mac", "macAddress"]))
}

fn client_display_name(client: &UniClient) -> Option<String> {
    client
        .display_name
        .clone()
        .or_else(|| client.name.clone())
        .or_else(|| client.hostname.clone())
        .or_else(|| extra_string(client, &["displayName", "name", "hostname"]))
}

fn client_ip(client: &UniClient) -> Option<String> {
    client
        .ip_address
        .clone()
        .or_else(|| extra_string(client, &["ipAddress", "ip"]))
}

fn client_mac(client: &UniClient) -> Option<String> {
    client
        .mac_address
        .clone()
        .or_else(|| extra_string(client, &["macAddress", "mac"]))
        .or_else(|| {
            client_client_id(client).and_then(|id| {
                if normalize_mac(&id).len() == 12 {
                    Some(id)
                } else {
                    None
                }
            })
        })
}

fn client_uplink_device_id(client: &UniClient) -> Option<String> {
    client
        .uplink_device_id
        .clone()
        .or_else(|| extra_string(client, &["uplinkDeviceId", "uplinkDeviceID"]))
}

fn client_uplink_port(client: &UniClient) -> Option<u32> {
    client
        .uplink_port
        .or_else(|| extra_u32(client, &["uplinkPort", "portIdx", "swPort", "switchPort"]))
}

fn legacy_display_name(client: &LegacyClient) -> Option<String> {
    client
        .name
        .clone()
        .or_else(|| client.hostname.clone())
        .filter(|value| !value.trim().is_empty())
}

fn legacy_ip(client: &LegacyClient) -> Option<String> {
    client
        .ip
        .clone()
        .or_else(|| client.last_ip.clone())
        .or_else(|| client.fixed_ip.clone())
        .filter(|value| !value.trim().is_empty())
}

fn legacy_network_name(client: &LegacyClient) -> Option<String> {
    client
        .network
        .clone()
        .or_else(|| client.last_connection_network_name.clone())
        .filter(|value| !value.trim().is_empty())
}

fn legacy_vlan(client: &LegacyClient) -> Option<u32> {
    client.vlan.or(client.gw_vlan)
}

fn extra_string(client: &UniClient, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| client.extra.get(*key))
        .filter_map(value_string)
        .next()
}

fn extra_u64(client: &UniClient, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .filter_map(|key| client.extra.get(*key))
        .filter_map(value_u64)
        .next()
}

fn extra_u32(client: &UniClient, keys: &[&str]) -> Option<u32> {
    extra_u64(client, keys).and_then(|v| u32::try_from(v).ok())
}

fn extra_i64(client: &UniClient, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .filter_map(|key| client.extra.get(*key))
        .filter_map(value_i64)
        .next()
}

fn value_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn value_u64(v: &serde_json::Value) -> Option<u64> {
    match v {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn value_i64(v: &serde_json::Value) -> Option<i64> {
    match v {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn rate_to_mbps(rate: u64) -> f64 {
    ((rate as f64 * 8.0 / 1_000_000.0) * 100.0).round() / 100.0
}

fn bytes_per_sec_to_mbps(rate: f64) -> f64 {
    ((rate * 8.0 / 1_000_000.0) * 100.0).round() / 100.0
}

fn epoch_to_rfc3339(value: i64) -> String {
    chrono::DateTime::from_timestamp(value, 0)
        .unwrap_or_default()
        .to_rfc3339()
}

fn mac_eq(a: &str, b: &str) -> bool {
    let a = normalize_mac(a);
    let b = normalize_mac(b);
    a.len() == 12 && b.len() == 12 && a == b
}

fn normalize_mac(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartNetworkScanReq {
    settings: Option<NetworkScannerSettings>,
    force: Option<bool>,
}

async fn start_network_scan(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StartNetworkScanReq>,
) -> ApiResult<serde_json::Value> {
    let scanner = req
        .settings
        .unwrap_or_else(|| state.config().network_scanner.clone())
        .normalized()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
    if !scanner.enabled && !req.force.unwrap_or(false) {
        return Err(bad_request("network scanner is disabled"));
    }
    let value = serde_json::to_value(&scanner).map_err(|e| anyhow::anyhow!(e))?;
    let id = db::enqueue_network_scan_job(&state.pool, "manual", &value).await?;
    Ok(Json(json!({ "id": id })))
}

async fn cancel_network_scan(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<serde_json::Value> {
    let canceled = db::cancel_network_scan_job(&state.pool, id).await?;
    if !canceled {
        return Err(bad_request("only queued network scan jobs can be canceled"));
    }
    Ok(Json(json!({ "ok": true })))
}

async fn start_host_port_scan(
    State(state): State<Arc<AppState>>,
    Path(target): Path<String>,
) -> ApiResult<serde_json::Value> {
    let target = target.trim().to_string();
    if target.is_empty() {
        return Err(bad_request("host target is required"));
    }
    let mut scanner = state.config().network_scanner.clone();
    scanner.enabled = true;
    scanner.ranges = vec![target];
    scanner.exclude.clear();
    scanner.port_scan.enabled = true;
    scanner.port_scan.only_scan_discovered = false;
    scanner.port_scan.skip_host_discovery = true;
    scanner.schedule.enabled = false;
    let value = settings_to_value(&scanner)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
    let id = db::enqueue_network_scan_job(&state.pool, "host-port-scan", &value).await?;
    Ok(Json(json!({ "id": id })))
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
struct UnraidSourceOut {
    id: i64,
    name: String,
    host: String,
    /// Whether an API key is stored — the key itself is never sent to clients.
    has_secret: bool,
    enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourcesResponse {
    unifi: Vec<UnifiSourceOut>,
    proxmox: Vec<ProxmoxSourceOut>,
    unraid: Vec<UnraidSourceOut>,
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
    let unraid = db::get_unraid_sources(&state.pool)
        .await?
        .into_iter()
        .map(|r| UnraidSourceOut {
            id: r.id,
            name: r.name,
            host: r.host,
            has_secret: !r.api_key.is_empty(),
            enabled: r.enabled,
        })
        .collect();
    Ok(Json(SourcesResponse {
        unifi,
        proxmox,
        unraid,
    }))
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
struct UnraidSourceIn {
    name: String,
    host: String,
    /// Omitted or empty on update keeps the stored key.
    api_key: Option<String>,
    enabled: Option<bool>,
}

async fn create_unraid(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UnraidSourceIn>,
) -> ApiResult<serde_json::Value> {
    let name = req.name.trim();
    let host = req.host.trim();
    if name.is_empty() || host.is_empty() {
        return Err(bad_request("name and host are required"));
    }
    let api_key = req.api_key.unwrap_or_default();
    if api_key.trim().is_empty() {
        return Err(bad_request("API key is required"));
    }
    let taken = db::get_unraid_sources(&state.pool)
        .await?
        .iter()
        .any(|r| r.name == name);
    if taken {
        return Err(bad_request(format!(
            "an Unraid source named '{name}' already exists"
        )));
    }
    let id = db::insert_unraid_source(
        &state.pool,
        name,
        host,
        api_key.trim(),
        req.enabled.unwrap_or(true),
    )
    .await?;
    refresh_config(&state).await?;
    Ok(Json(json!({ "id": id })))
}

async fn update_unraid(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<UnraidSourceIn>,
) -> ApiResult<serde_json::Value> {
    let existing = db::get_unraid_source(&state.pool, id)
        .await?
        .ok_or_else(|| not_found("unknown Unraid source"))?;
    let name = req.name.trim();
    let host = req.host.trim();
    if name.is_empty() || host.is_empty() {
        return Err(bad_request("name and host are required"));
    }
    let api_key = match req.api_key {
        Some(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ => existing.api_key,
    };
    db::update_unraid_source(
        &state.pool,
        id,
        name,
        host,
        &api_key,
        req.enabled.unwrap_or(existing.enabled),
    )
    .await?;
    refresh_config(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_unraid(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<serde_json::Value> {
    if !db::delete_unraid_source(&state.pool, id).await? {
        return Err(not_found("unknown Unraid source"));
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
        return Err(bad_request(format!(
            "a Proxmox source named '{name}' already exists"
        )));
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
                    detail: format!(
                        "UniFi Network {} · {} device(s)",
                        d.app_version,
                        d.devices.len()
                    ),
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
        "unraid" => {
            let api_key = match req.api_key {
                Some(k) if !k.trim().is_empty() => k.trim().to_string(),
                _ => match req.id {
                    Some(id) => {
                        db::get_unraid_source(&state.pool, id)
                            .await?
                            .ok_or_else(|| not_found("unknown Unraid source"))?
                            .api_key
                    }
                    None => return Err(bad_request("API key is required")),
                },
            };
            let client = UnraidClient::new(
                &UnraidConfig {
                    name: "test".to_string(),
                    host,
                    api_key,
                },
                timeout,
            )?;
            Ok(Json(match client.collect().await {
                Ok(d) => TestResult {
                    ok: true,
                    detail: format!(
                        "Unraid {} · array {} · {} container(s)",
                        d.version,
                        d.array.state,
                        d.containers.len()
                    ),
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
