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
    let mut next_bind_address = cfg.bind_address.clone();
    let mut next_port = cfg.port;

    if let Some(b) = req.bind_address {
        if !remote_addr.ip().is_loopback() {
            return Err(AppError::forbidden(
                "bind_address can only be updated from loopback clients",
            ));
        }
        if b.trim().is_empty() {
            return Err(AppError::bad_request("bind_address cannot be empty"));
        }
        next_bind_address = b;
    }
    if let Some(p) = req.port {
        if p == 0 {
            return Err(AppError::bad_request("port must be greater than 0"));
        }
        next_port = p;
    }

    format!("{next_bind_address}:{next_port}")
        .parse::<SocketAddr>()
        .map_err(|err| AppError::bad_request(format!("invalid bind_address or port: {err}")))?;

    cfg.bind_address = next_bind_address;
    cfg.port = next_port;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::DaemonConfig, db};
    use std::path::PathBuf;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
    }

    async fn test_state() -> AppState {
        let root = unique_temp_dir("aicli-settings-route-test");
        let data_dir = root.join("data");
        let web_dir = root.join("web");
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        std::fs::create_dir_all(&web_dir).expect("create web dir");
        std::fs::write(web_dir.join("index.html"), "<!doctype html><html></html>")
            .expect("write index");

        let cfg = DaemonConfig::for_tests(data_dir, web_dir, "test-token".to_string());
        let db = db::init_sqlite(&cfg.data_dir).await.expect("init sqlite");
        AppState::new(cfg, db)
    }

    #[tokio::test]
    async fn update_settings_rejects_invalid_bind_address() {
        let state = test_state().await;
        let addr: SocketAddr = "127.0.0.1:5000".parse().expect("parse socket addr");
        let err = update_settings(
            State(state),
            ConnectInfo(addr),
            Json(SettingsUpdateRequest {
                bind_address: Some("foo".to_string()),
                port: None,
            }),
        )
        .await
        .expect_err("invalid bind address should fail");
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn update_settings_accepts_valid_bind_address_and_port() {
        let state = test_state().await;
        let addr: SocketAddr = "127.0.0.1:5001".parse().expect("parse socket addr");
        let resp = update_settings(
            State(state),
            ConnectInfo(addr),
            Json(SettingsUpdateRequest {
                bind_address: Some("127.0.0.1".to_string()),
                port: Some(9876),
            }),
        )
        .await
        .expect("valid settings update should succeed")
        .0;

        assert_eq!(resp.bind_address, "127.0.0.1");
        assert_eq!(resp.port, 9876);
    }
}
