use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use base64::Engine;
use serde::Deserialize;
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

    let tail = state.process.tail_output(id, bytes).await?;

    Ok(Json(InstanceOutputTailEnvelope {
        id,
        bytes,
        encoding: "base64".to_string(),
        data: base64::engine::general_purpose::STANDARD.encode(tail.data),
        truncated: tail.truncated,
    }))
}
