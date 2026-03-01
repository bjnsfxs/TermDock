use axum::{
    extract::{ConnectInfo, State},
    routing::{get, post},
    Json, Router,
};
use std::net::SocketAddr;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{SettingsResponse, SettingsUpdateRequest, TokenRotateResponse},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(get_settings).put(update_settings))
        .route("/auth/token/rotate", post(rotate_token))
}

pub async fn get_settings(
    State(state): State<AppState>,
) -> Result<Json<SettingsResponse>, AppError> {
    let cfg = state.config_read();
    Ok(Json(SettingsResponse {
        bind_address: cfg.bind_address,
        port: cfg.port,
        data_dir: cfg.data_dir.display().to_string(),
        token: cfg.token,
    }))
}

pub async fn update_settings(
    State(state): State<AppState>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    Json(req): Json<SettingsUpdateRequest>,
) -> Result<Json<SettingsResponse>, AppError> {
    // NOTE: changing bind/port requires daemon restart to take effect.
    let mut cfg = state.config_write();

    if let Some(b) = req.bind_address {
        if !remote_addr.ip().is_loopback() {
            return Err(AppError::forbidden(
                "bind_address can only be updated from loopback clients",
            ));
        }
        if b.trim().is_empty() {
            return Err(AppError::bad_request("bind_address cannot be empty"));
        }
        cfg.bind_address = b;
    }
    if let Some(p) = req.port {
        if p == 0 {
            return Err(AppError::bad_request("port must be greater than 0"));
        }
        cfg.port = p;
    }

    cfg.save()?;

    Ok(Json(SettingsResponse {
        bind_address: cfg.bind_address.clone(),
        port: cfg.port,
        data_dir: cfg.data_dir.display().to_string(),
        token: cfg.token.clone(),
    }))
}

pub async fn rotate_token(
    State(state): State<AppState>,
) -> Result<Json<TokenRotateResponse>, AppError> {
    let mut cfg = state.config_write();
    cfg.token = Uuid::new_v4().to_string();
    cfg.save()?;
    Ok(Json(TokenRotateResponse {
        token: cfg.token.clone(),
    }))
}
