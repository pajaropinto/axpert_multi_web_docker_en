/// Session-based authentication.
///
/// Sessions are kept in memory (HashMap) with expiry timestamps.
/// On restart sessions are lost — users must log in again.
use axum::{
    extract::State,
    http::{header, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub const SESSION_COOKIE: &str = "axpert_session";

/// Role allowed to change configuration, send commands and manage users.
/// Any other role ("user") can only view live data.
pub const ROLE_ADMIN: &str = "admin";
pub const VALID_ROLES: [&str; 2] = [ROLE_ADMIN, "user"];

#[derive(Debug, Clone)]
pub struct Session {
    pub username: String,
    pub role: String,
    pub expires_at: chrono::DateTime<Utc>,
}

pub type SessionStore = Arc<RwLock<HashMap<String, Session>>>;

pub fn new_session_store() -> SessionStore {
    Arc::new(RwLock::new(HashMap::new()))
}

pub async fn create_session(
    sessions: &SessionStore,
    username: &str,
    role: &str,
    timeout_secs: u64,
) -> String {
    let session_id = Uuid::new_v4().to_string();
    let session = Session {
        username: username.to_string(),
        role: role.to_string(),
        expires_at: Utc::now() + Duration::seconds(timeout_secs as i64),
    };
    sessions.write().await.insert(session_id.clone(), session);
    session_id
}

pub async fn get_session(sessions: &SessionStore, session_id: &str) -> Option<Session> {
    let store = sessions.read().await;
    let session = store.get(session_id)?;
    if session.expires_at > Utc::now() {
        Some(session.clone())
    } else {
        None
    }
}

pub async fn remove_session(sessions: &SessionStore, session_id: &str) {
    sessions.write().await.remove(session_id);
}

/// Clean up expired sessions (call periodically).
pub async fn purge_expired_sessions(sessions: &SessionStore) {
    let now = Utc::now();
    sessions.write().await.retain(|_, s| s.expires_at > now);
}

// ─── Login/Logout handlers ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub role: Option<String>,
    pub message: String,
}

pub async fn handle_login(
    State(state): State<crate::web::AppState>,
    axum::Json(body): axum::Json<LoginRequest>,
) -> impl IntoResponse {
    let db = state.db.lock().await;
    match db.verify_user(&body.username, &body.password) {
        Ok(Some(user)) => {
            drop(db);
            let sid = create_session(
                &state.sessions,
                &user.username,
                &user.role,
                state.config.read().await.web.session_timeout_seconds,
            )
            .await;

            let cookie = format!(
                "{}={}; Path=/; HttpOnly; SameSite=Strict",
                SESSION_COOKIE, sid
            );
            let cookie_val = HeaderValue::from_str(&cookie)
                .unwrap_or_else(|_| HeaderValue::from_static(""));

            let mut resp = axum::Json(LoginResponse {
                success: true,
                role: Some(user.role),
                message: "Login successful".to_string(),
            })
            .into_response();
            resp.headers_mut().insert(header::SET_COOKIE, cookie_val);
            resp
        }
        Ok(None) => (
            StatusCode::UNAUTHORIZED,
            axum::Json(LoginResponse {
                success: false,
                role: None,
                message: "Invalid username or password".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(LoginResponse {
                    success: false,
                    role: None,
                    message: format!("Database error: {}", e),
                }),
            )
                .into_response()
        }
    }
}

pub async fn handle_logout(
    State(state): State<crate::web::AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if let Some(sid) = extract_session_id(&headers) {
        remove_session(&state.sessions, &sid).await;
    }
    let cookie = format!("{}=; Path=/; Max-Age=0", SESSION_COOKIE);
    let cookie_val = HeaderValue::from_str(&cookie)
        .unwrap_or_else(|_| HeaderValue::from_static(""));
    let mut resp = axum::Json(serde_json::json!({ "success": true })).into_response();
    resp.headers_mut().insert(header::SET_COOKIE, cookie_val);
    resp
}

/// Extract session ID from Cookie header.
pub fn extract_session_id(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_str = headers.get("cookie")?.to_str().ok()?;
    for part in cookie_str.split(';') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix(&format!("{}=", SESSION_COOKIE)) {
            return Some(val.to_string());
        }
    }
    None
}

impl Session {
    pub fn is_admin(&self) -> bool {
        self.role == ROLE_ADMIN
    }
}

/// Axum middleware: require a valid session, else redirect to /login.html.
/// The session is stored in the request extensions for later handlers.
pub async fn require_auth(
    State(state): State<crate::web::AppState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    // Allow public paths without auth
    let public = ["/api/login", "/api/health", "/login.html", "/style.css", "/login.js"];
    if public.iter().any(|p| path.starts_with(p)) {
        return next.run(request).await;
    }

    let headers = request.headers().clone();
    if let Some(sid) = extract_session_id(&headers) {
        if let Some(session) = get_session(&state.sessions, &sid).await {
            request.extensions_mut().insert(session);
            return next.run(request).await;
        }
    }

    // Not authenticated
    if path.starts_with("/api/") || path.starts_with("/ws") {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    // Redirect to login
    axum::response::Redirect::to("/login.html").into_response()
}

/// Axum middleware for admin-only routes. Must run after `require_auth`.
pub async fn require_admin(request: Request<axum::body::Body>, next: Next) -> Response {
    match request.extensions().get::<Session>() {
        Some(s) if s.is_admin() => next.run(request).await,
        _ => (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "success": false, "message": "Admin role required" })),
        )
            .into_response(),
    }
}

/// GET /api/me — current user and role, so the UI can hide admin-only parts.
pub async fn handle_me(axum::Extension(session): axum::Extension<Session>) -> impl IntoResponse {
    axum::Json(serde_json::json!({ "username": session.username, "role": session.role }))
}
