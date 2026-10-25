/// REST API handlers.
use crate::config::{AppConfig, InverterParamConfig};
use crate::database::UserRecord;
use crate::inverter::{config_cmds, manager as inv_manager};
use crate::web::auth::{Session, VALID_ROLES};
use crate::web::AppState;
use axum::{
    extract::{Path, State},
    Extension,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{error, info};

// ─── System status ────────────────────────────────────────────────────────────

pub async fn get_status(State(state): State<AppState>) -> impl IntoResponse {
    let snap = state.latest_snapshot.read().await;
    match snap.as_ref() {
        Some(s) => Json(json!({
            "inverters": s.inverters.len(),
            "system_status": s.totals.system_status,
            "inverters_online": s.totals.inverters_online,
        })),
        None => Json(json!({ "status": "no_data" })),
    }
}

/// Unauthenticated liveness probe for the Docker healthcheck.
pub async fn get_health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

pub async fn get_snapshot(State(state): State<AppState>) -> impl IntoResponse {
    let snap = state.latest_snapshot.read().await;
    match snap.as_ref() {
        Some(s) => (StatusCode::OK, Json(json!(s))).into_response(),
        None => (StatusCode::NO_CONTENT, "No data yet").into_response(),
    }
}

// ─── App config ───────────────────────────────────────────────────────────────

pub async fn get_app_config(State(state): State<AppState>) -> impl IntoResponse {
    let cfg = state.config.read().await;
    Json(json!(&*cfg))
}

pub async fn save_app_config(
    State(_state): State<AppState>,
    Json(cfg): Json<AppConfig>,
) -> impl IntoResponse {
    if let Err(e) = cfg.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "message": format!("Invalid configuration: {}", e) })),
        );
    }
    match toml::to_string_pretty(&cfg) {
        Ok(toml_str) => {
            if let Err(e) = crate::config::write_atomic(crate::config::watcher::CONFIG_PATH, &toml_str) {
                error!("Cannot write config: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "success": false, "message": format!("Write error: {}", e) })),
                );
            }
            info!("App config saved — watcher will reload");
            (StatusCode::OK, Json(json!({ "success": true, "message": "Config saved. Reloading..." })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "message": format!("Serialize error: {}", e) })),
        ),
    }
}

// ─── Inverter config ──────────────────────────────────────────────────────────

pub async fn get_inverter_config(Path(id): Path<u8>) -> impl IntoResponse {
    match InverterParamConfig::load(id) {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "success": false, "message": format!("Cannot load config: {}", e) })),
        )
            .into_response(),
    }
}

pub async fn save_inverter_config(
    Path(id): Path<u8>,
    Json(cfg): Json<InverterParamConfig>,
) -> impl IntoResponse {
    match cfg.save(id) {
        Ok(_) => {
            info!("Inverter {} config saved", id);
            (StatusCode::OK, Json(json!({ "success": true, "message": "Config saved" })))
        }
        Err(e) => {
            error!("Cannot save inverter {} config: {}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "success": false, "message": format!("Save error: {}", e) })),
            )
        }
    }
}

// ─── Inverter commands ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SendCommandRequest {
    pub inv_id: u8,
    pub param: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct CommandResponse {
    pub success: bool,
    pub message: String,
}

pub async fn send_inverter_command(
    State(state): State<AppState>,
    Json(req): Json<SendCommandRequest>,
) -> impl IntoResponse {
    let cfg = state.config.read().await;

    let inv = match cfg
        .active_inverters()
        .into_iter()
        .find(|i| i.id == req.inv_id)
    {
        Some(i) => i.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(CommandResponse {
                    success: false,
                    message: format!("Inverter {} not found", req.inv_id),
                }),
            );
        }
    };
    drop(cfg);

    // Build command string from param + value
    let cmd_result = match req.param.as_str() {
        "source_priority" => config_cmds::cmd_output_source_priority(&req.value),
        "ac_input_range" => config_cmds::cmd_ac_input_range(&req.value),
        "output_frequency" => config_cmds::cmd_output_frequency(&req.value),
        "max_charger_current" => config_cmds::cmd_max_charger_current(req.inv_id, &req.value),
        "charger_priority" => config_cmds::cmd_charger_priority(req.inv_id, &req.value),
        "max_discharge_current" => config_cmds::cmd_max_discharge_current(&req.value),
        "overload_restart" => config_cmds::cmd_overload_restart(&req.value),
        "overtemp_restart" => config_cmds::cmd_overtemp_restart(&req.value),
        "buzzer" => config_cmds::cmd_buzzer(&req.value),
        "lcd_escape" => config_cmds::cmd_lcd_escape(&req.value),
        "backlight" => config_cmds::cmd_backlight(&req.value),
        "grid_loss_alarm" => config_cmds::cmd_grid_loss_alarm(&req.value),
        "overload_bypass" => config_cmds::cmd_overload_bypass(&req.value),
        "fault_logging" => config_cmds::cmd_fault_logging(&req.value),
        "equalization" => config_cmds::cmd_equalization(&req.value),
        other => {
            return (
                StatusCode::BAD_REQUEST,
                Json(CommandResponse {
                    success: false,
                    message: format!("Unknown parameter: {}", other),
                }),
            );
        }
    };

    let cmd = match cmd_result {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(CommandResponse {
                    success: false,
                    message: format!("Invalid value: {}", e),
                }),
            );
        }
    };

    info!(
        "Sending command '{}' to inverter {} ({}:{})",
        cmd, inv.id, inv.tcp_ip, inv.tcp_port
    );

    match inv_manager::send_config_command(&inv.tcp_ip, inv.tcp_port, &cmd).await {
        Ok(true) => (
            StatusCode::OK,
            Json(CommandResponse {
                success: true,
                message: format!("Command '{}' accepted (ACK)", cmd),
            }),
        ),
        Ok(false) => (
            StatusCode::OK,
            Json(CommandResponse {
                success: false,
                message: format!("Command '{}' rejected (NAK) — check value", cmd),
            }),
        ),
        Err(e) => {
            error!("Command send error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(CommandResponse {
                    success: false,
                    message: format!("Communication error: {}", e),
                }),
            )
        }
    }
}

/// Send a raw read command to an inverter and return the raw response.
#[derive(Deserialize)]
pub struct ReadCommandRequest {
    pub inv_id: u8,
    pub command: String,
}

pub async fn send_read_command(
    State(state): State<AppState>,
    Json(req): Json<ReadCommandRequest>,
) -> impl IntoResponse {
    let valid_read = [
        config_cmds::CMD_QID,
        config_cmds::CMD_QPIGS,
        config_cmds::CMD_QPIGS2,
        config_cmds::CMD_QFLAG,
        config_cmds::CMD_QPIRI,
        config_cmds::CMD_QPIWS,
        config_cmds::CMD_QMOD,
    ];
    if !valid_read.contains(&req.command.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "message": "Invalid read command" })),
        )
            .into_response();
    }

    let cfg = state.config.read().await;
    let inv = match cfg
        .active_inverters()
        .into_iter()
        .find(|i| i.id == req.inv_id)
    {
        Some(i) => i.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "success": false, "message": "Inverter not found" })),
            )
                .into_response();
        }
    };
    drop(cfg);

    match crate::inverter::protocol::connect(&inv.tcp_ip, inv.tcp_port).await {
        Ok(mut stream) => {
            match crate::inverter::protocol::send_command(&mut stream, &req.command).await {
                Ok(resp) => (
                    StatusCode::OK,
                    Json(json!({ "success": true, "response": resp })),
                )
                    .into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "success": false, "message": format!("Read error: {}", e) })),
                )
                    .into_response(),
            }
        }
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "success": false, "message": format!("Cannot connect: {}", e) })),
        )
            .into_response(),
    }
}

// ─── User management ──────────────────────────────────────────────────────────

pub async fn list_users(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.lock().await;
    match db.list_users() {
        Ok(users) => {
            let safe: Vec<Value> = users
                .iter()
                .map(|u| json!({ "username": u.username, "role": u.role }))
                .collect();
            (StatusCode::OK, Json(json!(safe))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "message": format!("{}", e) })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
}

pub async fn create_user(
    State(state): State<AppState>,
    Json(req): Json<CreateUserRequest>,
) -> impl IntoResponse {
    if !VALID_ROLES.contains(&req.role.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "message": "Role must be 'admin' or 'user'" })),
        )
            .into_response();
    }
    if req.username.is_empty() || req.password.len() < 4 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "message": "Username required; password min 4 chars" })),
        )
            .into_response();
    }

    let hash = match bcrypt::hash(&req.password, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "success": false, "message": format!("Hash error: {}", e) })),
            )
                .into_response();
        }
    };

    let db = state.db.lock().await;
    match db.save_user(&UserRecord {
        username: req.username.clone(),
        password_hash: hash,
        role: req.role,
    }) {
        Ok(_) => {
            info!("User '{}' created", req.username);
            (StatusCode::OK, Json(json!({ "success": true }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "message": format!("{}", e) })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub password: String,
}

pub async fn change_password(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(username): Path<String>,
    Json(req): Json<ChangePasswordRequest>,
) -> impl IntoResponse {
    if !session.is_admin() && session.username != username {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "success": false, "message": "You can only change your own password" })),
        )
            .into_response();
    }
    if req.password.len() < 4 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "message": "Password min 4 chars" })),
        )
            .into_response();
    }

    let db = state.db.lock().await;
    match db.change_password(&username, &req.password) {
        Ok(true) => {
            info!("Password changed for '{}'", username);
            (StatusCode::OK, Json(json!({ "success": true }))).into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "success": false, "message": "User not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "message": format!("{}", e) })),
        )
            .into_response(),
    }
}

// ─── Energy reset ─────────────────────────────────────────────────────────────

pub async fn reset_daily_energy(State(state): State<AppState>) -> impl IntoResponse {
    let cfg = state.config.read().await;
    let ids: Vec<u8> = cfg.active_inverters().iter().map(|i| i.id).collect();
    drop(cfg);

    let db = state.db.lock().await;
    match db.reset_daily(&ids) {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true, "message": "Daily counters reset" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "message": format!("{}", e) })),
        ).into_response(),
    }
}

pub async fn reset_all_energy(State(state): State<AppState>) -> impl IntoResponse {
    let cfg = state.config.read().await;
    let ids: Vec<u8> = cfg.active_inverters().iter().map(|i| i.id).collect();
    drop(cfg);

    let db = state.db.lock().await;
    match db.reset_all(&ids) {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true, "message": "All counters reset" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "message": format!("{}", e) })),
        ).into_response(),
    }
}

pub async fn delete_user(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    if username == session.username {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "success": false, "message": "Cannot delete your own user" })),
        )
            .into_response();
    }
    if username == "admin" {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "success": false, "message": "Cannot delete admin" })),
        )
            .into_response();
    }

    let db = state.db.lock().await;
    match db.delete_user(&username) {
        Ok(true) => {
            info!("User '{}' deleted", username);
            (StatusCode::OK, Json(json!({ "success": true }))).into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "success": false, "message": "User not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "message": format!("{}", e) })),
        )
            .into_response(),
    }
}
