use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{
        config_mode_to_db, now_rfc3339, parse_config_mode_db, InstanceConfig,
        InstanceConfigEnvelope, InstanceConfigUpdateRequest, InstanceRow,
    },
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct IncludeContentQuery {
    #[serde(default = "default_true")]
    pub include_content: bool,
}

fn default_true() -> bool {
    true
}

pub fn router() -> Router<AppState> {
    Router::new().route("/instances/:id/config", get(get_config).put(update_config))
}

pub async fn get_config(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<IncludeContentQuery>,
) -> Result<Json<InstanceConfigEnvelope>, AppError> {
    let row = get_row(&state, id).await?;
    let mode = parse_config_mode_db(&row.config_mode);

    let mut cfg = InstanceConfig {
        mode: mode.clone(),
        path: row.config_path.clone(),
        filename: row.config_filename.clone(),
        content: None,
    };

    if q.include_content {
        match mode {
            crate::models::ConfigMode::Inline => cfg.content = row.config_content.clone(),
            crate::models::ConfigMode::Path => {
                // Optional future behavior: read from file.
                // Skeleton returns None to avoid filesystem surprises.
                cfg.content = None;
            }
            crate::models::ConfigMode::None => cfg.content = None,
        }
    }

    Ok(Json(InstanceConfigEnvelope { id, config: cfg }))
}

pub async fn update_config(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<InstanceConfigUpdateRequest>,
) -> Result<Json<InstanceConfigEnvelope>, AppError> {
    validate(&req)?;

    let now = now_rfc3339();

    sqlx::query(
        r#"
        UPDATE instances SET
          config_mode=?,
          config_path=?,
          config_filename=?,
          config_content=?,
          updated_at=?
        WHERE id=?
        "#,
    )
    .bind(config_mode_to_db(&req.mode))
    .bind(req.path.clone())
    .bind(req.filename.clone())
    .bind(req.content.clone())
    .bind(now)
    .bind(id.to_string())
    .execute(&state.db)
    .await?;

    // Return view (include_content = true)
    let row = get_row(&state, id).await?;
    let mode = parse_config_mode_db(&row.config_mode);
    let cfg = InstanceConfig {
        mode: mode.clone(),
        path: row.config_path.clone(),
        filename: row.config_filename.clone(),
        content: match mode {
            crate::models::ConfigMode::Inline => row.config_content.clone(),
            _ => None,
        },
    };

    Ok(Json(InstanceConfigEnvelope { id, config: cfg }))
}

async fn get_row(state: &AppState, id: Uuid) -> Result<InstanceRow, AppError> {
    let row: Option<InstanceRow> =
        sqlx::query_as::<_, InstanceRow>(r#"SELECT * FROM instances WHERE id=? "#)
            .bind(id.to_string())
            .fetch_optional(&state.db)
            .await?;

    row.ok_or_else(|| AppError::not_found("instance not found"))
}

fn validate(req: &InstanceConfigUpdateRequest) -> Result<(), AppError> {
    match req.mode {
        crate::models::ConfigMode::None => Ok(()),
        crate::models::ConfigMode::Path => {
            if req.path.as_deref().unwrap_or("").trim().is_empty() {
                Err(AppError::bad_request(
                    "path required when mode=path (InstanceConfigUpdateRequest.path)",
                ))
            } else {
                Ok(())
            }
        }
        crate::models::ConfigMode::Inline => {
            if req.filename.as_deref().unwrap_or("").trim().is_empty() {
                return Err(AppError::bad_request(
                    "filename required when mode=inline (InstanceConfigUpdateRequest.filename)",
                ));
            }
            if req.content.as_deref().unwrap_or("").is_empty() {
                return Err(AppError::bad_request(
                    "content required when mode=inline (InstanceConfigUpdateRequest.content)",
                ));
            }
            Ok(())
        }
    }
}
