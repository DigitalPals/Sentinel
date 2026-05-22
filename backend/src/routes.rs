//! HTTP surface: the JSON API plus the bundled single-page frontend.

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::engine::{patch_alerts, AppState};

pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/api/snapshot", get(snapshot))
        .route("/api/health", get(health))
        .route("/api/alerts/action", post(alert_action))
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

/// The full monitoring snapshot consumed by the frontend.
async fn snapshot(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.current())
}

/// Source connectivity summary.
async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let s = state.current();
    Json(serde_json::json!({
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
        (StatusCode::OK, "ok")
    } else {
        (StatusCode::NOT_FOUND, "unknown alert")
    }
}

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
