use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use base64::Engine;
use serde::Deserialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{error::AppError, models::InstanceOutputTailEnvelope, state::AppState};

#[derive(Debug, Deserialize)]
pub struct OutputQuery {
    #[serde(default = "default_bytes")]
    pub bytes: usize,
    #[serde(default = "default_encoding")]
    pub encoding: String,
}

fn default_bytes() -> usize {
    8192
}

fn default_encoding() -> String {
    "base64".to_string()
}

pub fn router() -> Router<AppState> {
    Router::new().route("/instances/:id/output", get(get_output_tail))
}

pub async fn get_output_tail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<OutputQuery>,
) -> Result<Json<InstanceOutputTailEnvelope>, AppError> {
    if q.encoding != "base64" {
        return Err(AppError::bad_request("only encoding=base64 is supported"));
    }

    let bytes = q.bytes.clamp(1, 1024 * 1024);
    ensure_instance_exists(&state.db, id).await?;

    let tail = state.process.tail_output(id, bytes).await?;

    Ok(Json(InstanceOutputTailEnvelope {
        id,
        bytes,
        encoding: "base64".to_string(),
        data: base64::engine::general_purpose::STANDARD.encode(tail.data),
        truncated: tail.truncated,
    }))
}

async fn ensure_instance_exists(pool: &SqlitePool, id: Uuid) -> Result<(), AppError> {
    let row: Option<String> = sqlx::query_scalar(r#"SELECT id FROM instances WHERE id=? "#)
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
    if row.is_none() {
        return Err(AppError::not_found("instance not found"));
    }
    Ok(())
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
        let root = unique_temp_dir("aicli-output-route-test");
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
    async fn get_output_tail_returns_not_found_for_missing_instance() {
        let state = test_state().await;
        let id = Uuid::new_v4();

        let err = get_output_tail(
            State(state),
            Path(id),
            Query(OutputQuery {
                bytes: 8192,
                encoding: "base64".to_string(),
            }),
        )
        .await
        .expect_err("missing instance should return not found");
        assert!(matches!(err, AppError::NotFound(_)));
    }
}
