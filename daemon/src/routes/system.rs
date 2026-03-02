use axum::{
    extract::{ConnectInfo, State},
    routing::post,
    Json, Router,
};
use std::net::SocketAddr;

use crate::{error::AppError, models::ShutdownResponse, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/system/shutdown", post(shutdown))
}

pub async fn shutdown(
    State(state): State<AppState>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
) -> Result<Json<ShutdownResponse>, AppError> {
    if !remote_addr.ip().is_loopback() {
        return Err(AppError::forbidden(
            "shutdown can only be requested from loopback clients",
        ));
    }

    state.request_shutdown();
    Ok(Json(ShutdownResponse {
        status: "shutting-down".to_string(),
    }))
}
