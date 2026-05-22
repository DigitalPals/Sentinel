//! User authentication.
//!
//! Sentinel runs behind a single administrator account. The first user is
//! created during onboarding (the public `setup` endpoint, which disables
//! itself once a user exists); after that every `/api/*` route except the auth
//! endpoints is gated by [`require_session`].
//!
//! Sessions are opaque random tokens stored in an HttpOnly cookie. Only the
//! SHA-256 of a token is kept in the database, so a database or backup leak
//! never exposes a usable session. The login state lives in PostgreSQL, which
//! makes logout and expiry a simple row delete — consistent with the rest of
//! the app treating the database as the single source of truth.

use std::sync::Arc;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::db;
use crate::engine::AppState;
use crate::routes::{ApiError, ApiResult};

/// Name of the session cookie.
const SESSION_COOKIE: &str = "sentinel_session";
/// How long a login session stays valid.
const SESSION_TTL_DAYS: i64 = 30;
/// Minimum accepted password length.
const MIN_PASSWORD_LEN: usize = 8;

/// The authenticated user, attached to a request by [`require_session`] so
/// downstream handlers can tell who is acting (e.g. an alert assignee).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
}

// ── Password & token primitives ─────────────────────────────────────────────

/// Hash a password with Argon2id, returning a self-describing PHC string.
fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("hashing password: {e}"))
}

/// Check a candidate password against a stored Argon2 PHC string.
fn verify_password(stored_hash: &str, candidate: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(candidate.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// A fresh, unguessable session token — the value placed in the cookie.
fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    to_hex(&bytes)
}

/// SHA-256 of a raw token. Only this is persisted, so the database never holds
/// a token that could be replayed.
fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    to_hex(&hasher.finalize())
}

// ── Cookies ─────────────────────────────────────────────────────────────────

/// Build the session cookie. `Secure` is opt-in via `SENTINEL_SECURE_COOKIES`,
/// off by default so the common plain-HTTP LAN deployment still works.
fn session_cookie(token: String) -> Cookie<'static> {
    let mut c = Cookie::new(SESSION_COOKIE, token);
    c.set_http_only(true);
    c.set_same_site(SameSite::Lax);
    c.set_path("/");
    c.set_max_age(time::Duration::days(SESSION_TTL_DAYS));
    if std::env::var("SENTINEL_SECURE_COOKIES").is_ok() {
        c.set_secure(true);
    }
    c
}

/// A cookie that, once sent, clears the session cookie in the browser.
fn clearing_cookie() -> Cookie<'static> {
    let mut c = Cookie::new(SESSION_COOKIE, "");
    c.set_path("/");
    c.set_max_age(time::Duration::ZERO);
    c
}

fn unauthorized() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "authentication required".into())
}

/// Persist a new session for `user_id` and return the cookie carrying it.
async fn open_session(state: &AppState, user_id: i64) -> Result<Cookie<'static>, ApiError> {
    let token = generate_token();
    let expires_at = Utc::now() + chrono::Duration::days(SESSION_TTL_DAYS);
    db::create_session(&state.pool, &hash_token(&token), user_id, expires_at).await?;
    Ok(session_cookie(token))
}

// ── Middleware ──────────────────────────────────────────────────────────────

/// Gate a request on a valid, unexpired session. Applied to every protected
/// API route; rejects with `401` otherwise.
pub async fn require_session(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let token = jar
        .get(SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or_else(unauthorized)?;
    let token_hash = hash_token(&token);

    let session = match db::get_session(&state.pool, &token_hash).await? {
        Some(s) if s.expires_at > Utc::now() => s,
        Some(_) => {
            // Expired — drop the stale row so the table stays small.
            let _ = db::delete_session(&state.pool, &token_hash).await;
            return Err(unauthorized());
        }
        None => return Err(unauthorized()),
    };

    req.extensions_mut().insert(CurrentUser {
        id: session.user_id,
        username: session.username,
    });
    Ok(next.run(req).await)
}

// ── Handlers ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    /// No account exists yet — the frontend should show first-user setup.
    needs_first_user: bool,
    /// The current request carries a valid session.
    authenticated: bool,
    username: Option<String>,
}

/// Public: lets the frontend decide between first-user setup, the login screen
/// and the app. The auth middleware does not run here, so it reads the cookie
/// itself.
pub async fn auth_status(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> ApiResult<AuthStatus> {
    let needs_first_user = db::count_users(&state.pool).await? == 0;

    let mut authenticated = false;
    let mut username = None;
    if let Some(c) = jar.get(SESSION_COOKIE) {
        if let Some(s) = db::get_session(&state.pool, &hash_token(c.value())).await? {
            if s.expires_at > Utc::now() {
                authenticated = true;
                username = Some(s.username);
            }
        }
    }

    Ok(Json(AuthStatus {
        needs_first_user,
        authenticated,
        username,
    }))
}

#[derive(Deserialize)]
pub(crate) struct Credentials {
    username: String,
    password: String,
}

/// Public bootstrap: create the first (and only) administrator account. Refuses
/// once any user exists, so it is safe to leave un-gated. Logs the new user
/// straight in by returning a session cookie.
pub async fn auth_setup(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(req): Json<Credentials>,
) -> Result<(CookieJar, Json<Value>), ApiError> {
    if db::count_users(&state.pool).await? > 0 {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "setup has already been completed".into(),
        ));
    }
    let username = req.username.trim();
    if username.is_empty() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "a username is required".into(),
        ));
    }
    if req.password.len() < MIN_PASSWORD_LEN {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            format!("password must be at least {MIN_PASSWORD_LEN} characters"),
        ));
    }

    let hash = hash_password(&req.password)?;
    // The unique index on lower(username) is the final guard against a racing
    // second setup request; the count check above handles the common case.
    let user_id = db::insert_user(&state.pool, username, &hash).await?;
    let cookie = open_session(&state, user_id).await?;
    Ok((jar.add(cookie), Json(json!({ "username": username }))))
}

/// Public: verify credentials and start a session.
pub async fn auth_login(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(req): Json<Credentials>,
) -> Result<(CookieJar, Json<Value>), ApiError> {
    let user = db::get_user_by_username(&state.pool, req.username.trim()).await?;
    let valid = user
        .as_ref()
        .map(|u| verify_password(&u.password_hash, &req.password))
        .unwrap_or(false);
    if !valid {
        // Same message whether the user is unknown or the password is wrong.
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "invalid username or password".into(),
        ));
    }

    let user = user.expect("validated above");
    let cookie = open_session(&state, user.id).await?;
    Ok((jar.add(cookie), Json(json!({ "username": user.username }))))
}

/// Public: end the current session. Idempotent — clearing an absent or stale
/// session still returns `200`.
pub async fn auth_logout(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> (CookieJar, Json<Value>) {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        let _ = db::delete_session(&state.pool, &hash_token(c.value())).await;
    }
    (jar.add(clearing_cookie()), Json(json!({ "ok": true })))
}
