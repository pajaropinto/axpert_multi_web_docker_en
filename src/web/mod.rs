pub mod api;
pub mod auth;
pub mod ws;

use crate::config::AppConfig;
use crate::database::SharedDb;
use crate::mqtt::publisher::SystemSnapshot;
use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use axum::http::{header, HeaderValue};
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

/// Shared application state available to all axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    pub db: SharedDb,
    pub sessions: auth::SessionStore,
    pub latest_snapshot: Arc<RwLock<Option<SystemSnapshot>>>,
    pub snapshot_tx: broadcast::Sender<SystemSnapshot>,
}

pub fn build_router(state: AppState) -> Router {
    // Routes that change the system or expose credentials: admin role only
    let admin_routes = Router::new()
        // App config
        .route("/config/app", get(api::get_app_config))
        .route("/config/app", put(api::save_app_config))
        // Inverter config
        .route("/config/inverter/:id", get(api::get_inverter_config))
        .route("/config/inverter/:id", put(api::save_inverter_config))
        // Inverter commands
        .route("/command", post(api::send_inverter_command))
        .route("/read", post(api::send_read_command))
        // Energy reset
        .route("/energy/reset/daily", post(api::reset_daily_energy))
        .route("/energy/reset/all", post(api::reset_all_energy))
        // User management
        .route("/users", get(api::list_users))
        .route("/users", post(api::create_user))
        .route("/users/:username", delete(api::delete_user))
        .route_layer(middleware::from_fn(auth::require_admin));

    let api_routes = Router::new()
        // Auth
        .route("/login", post(auth::handle_login))
        .route("/logout", post(auth::handle_logout))
        // System
        .route("/status", get(api::get_status))
        .route("/health", get(api::get_health))
        .route("/snapshot", get(api::get_snapshot))
        .route("/me", get(auth::handle_me))
        // Any user may change their own password (admins: anyone's)
        .route("/users/:username/password", put(api::change_password))
        .merge(admin_routes)
        .with_state(state.clone());

    Router::new()
        .nest("/api", api_routes)
        .route("/ws", get(ws::ws_handler))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        // no-cache: browsers revalidate the web UI files (cheap 304 when
        // unchanged), so after an image update they never run an old app.js
        .fallback_service(
            ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::overriding(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("no-cache"),
                ))
                .service(ServeDir::new("www").append_index_html_on_directories(true)),
        )
}

pub async fn start(state: AppState, port: u16) -> anyhow::Result<()> {
    let router = build_router(state);
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Web server listening on http://{}", addr);
    axum::serve(listener, router).await?;
    Ok(())
}
