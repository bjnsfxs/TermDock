use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{
        config_mode_to_db, now_rfc3339, parse_config_mode_db, parse_restart_policy_db,
        restart_policy_to_db, Instance, InstanceCreateRequest, InstanceEnvelope,
        InstanceListEnvelope, InstanceRow, InstanceRuntimeEnvelope, InstanceUpdateRequest,
    },
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct IncludeRuntimeQuery {
    #[serde(default = "default_true")]
    pub include_runtime: bool,
}

fn default_true() -> bool {
    true
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/instances", get(list_instances).post(create_instance))
        .route(
            "/instances/:id",
            get(get_instance)
                .put(update_instance)
                .delete(delete_instance),
        )
        .route("/instances/:id/start", post(start_instance))
        .route("/instances/:id/stop", post(stop_instance))
        .route("/instances/:id/restart", post(restart_instance))
}

pub async fn list_instances(
    State(state): State<AppState>,
    Query(q): Query<IncludeRuntimeQuery>,
) -> Result<Json<InstanceListEnvelope>, AppError> {
    let rows: Vec<InstanceRow> =
        sqlx::query_as::<_, InstanceRow>(r#"SELECT * FROM instances ORDER BY created_at DESC"#)
            .fetch_all(&state.db)
            .await?;

    let mut instances = Vec::with_capacity(rows.len());
    for row in rows {
        let id = Uuid::parse_str(&row.id).map_err(|_| AppError::internal("invalid uuid in db"))?;
        let runtime = if q.include_runtime {
            state.process.runtime(id).await
        } else {
            None
        };
        instances.push(row.to_instance(runtime)?);
    }

    Ok(Json(InstanceListEnvelope { instances }))
}

pub async fn create_instance(
    State(state): State<AppState>,
    Json(req): Json<InstanceCreateRequest>,
) -> Result<(StatusCode, Json<InstanceEnvelope>), AppError> {
    validate_create(&req)?;

    let id = Uuid::new_v4();
    let now = now_rfc3339();

    let args_json = serde_json::to_string(&req.args)?;
    let env_json = serde_json::to_string(&req.env)?;

    sqlx::query(
        r#"
        INSERT INTO instances (
          id, name, enabled,
          command, args_json, cwd, env_json, use_pty,
          config_mode, config_path, config_filename, config_content,
          restart_policy, auto_start,
          created_at, updated_at
        )
        VALUES (?, ?, ?,
                ?, ?, ?, ?, ?,
                ?, ?, ?, ?,
                ?, ?,
                ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(req.name)
    .bind(bool_to_i64(req.enabled))
    .bind(req.command)
    .bind(args_json)
    .bind(req.cwd)
    .bind(env_json)
    .bind(bool_to_i64(req.use_pty))
    .bind(config_mode_to_db(&req.config_mode))
    .bind(req.config_path)
    .bind(req.config_filename)
    .bind(req.config_content)
    .bind(restart_policy_to_db(&req.restart_policy))
    .bind(bool_to_i64(req.auto_start))
    .bind(now.clone())
    .bind(now.clone())
    .execute(&state.db)
    .await?;

    let created = get_instance_by_id(&state.db, id).await?;
    Ok((
        StatusCode::CREATED,
        Json(InstanceEnvelope { instance: created }),
    ))
}

pub async fn get_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<IncludeRuntimeQuery>,
) -> Result<Json<InstanceEnvelope>, AppError> {
    let instance = get_instance_by_id(&state.db, id).await?;
    let runtime = if q.include_runtime {
        state.process.runtime(id).await
    } else {
        None
    };

    Ok(Json(InstanceEnvelope {
        instance: Instance {
            runtime,
            ..instance
        },
    }))
}

pub async fn update_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(patch): Json<InstanceUpdateRequest>,
) -> Result<Json<InstanceEnvelope>, AppError> {
    let mut existing = get_instance_by_id(&state.db, id).await?;

    apply_patch(&mut existing, patch)?;

    let now = now_rfc3339();
    existing.updated_at = now.clone();

    let args_json = serde_json::to_string(&existing.args)?;
    let env_json = serde_json::to_string(&existing.env)?;

    sqlx::query(
        r#"
        UPDATE instances SET
          name=?,
          enabled=?,
          command=?,
          args_json=?,
          cwd=?,
          env_json=?,
          use_pty=?,
          config_mode=?,
          config_path=?,
          config_filename=?,
          config_content=?,
          restart_policy=?,
          auto_start=?,
          updated_at=?
        WHERE id=?
        "#,
    )
    .bind(existing.name.clone())
    .bind(bool_to_i64(existing.enabled))
    .bind(existing.command.clone())
    .bind(args_json)
    .bind(existing.cwd.clone())
    .bind(env_json)
    .bind(bool_to_i64(existing.use_pty))
    .bind(config_mode_to_db(&existing.config_mode))
    .bind(existing.config_path.clone())
    .bind(existing.config_filename.clone())
    .bind(existing.config_content.clone())
    .bind(restart_policy_to_db(&existing.restart_policy))
    .bind(bool_to_i64(existing.auto_start))
    .bind(now)
    .bind(id.to_string())
    .execute(&state.db)
    .await?;

    let updated = get_instance_by_id(&state.db, id).await?;
    Ok(Json(InstanceEnvelope { instance: updated }))
}

pub async fn delete_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let _ = get_instance_by_id(&state.db, id).await?;

    // Best effort: stop if running.
    let _ = state.process.stop(id).await;

    let res = sqlx::query(r#"DELETE FROM instances WHERE id=? "#)
        .bind(id.to_string())
        .execute(&state.db)
        .await?;

    if res.rows_affected() == 0 {
        return Err(AppError::not_found("instance not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn start_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<InstanceRuntimeEnvelope>, AppError> {
    let instance = get_instance_by_id(&state.db, id).await?;
    let rt = state.process.start(&instance).await?;
    Ok(Json(InstanceRuntimeEnvelope { id, runtime: rt }))
}

pub async fn stop_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<InstanceRuntimeEnvelope>, AppError> {
    let _ = get_instance_by_id(&state.db, id).await?;
    let rt = state.process.stop(id).await?;
    Ok(Json(InstanceRuntimeEnvelope { id, runtime: rt }))
}

pub async fn restart_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<InstanceRuntimeEnvelope>, AppError> {
    let instance = get_instance_by_id(&state.db, id).await?;
    let rt = state.process.restart(&instance).await?;
    Ok(Json(InstanceRuntimeEnvelope { id, runtime: rt }))
}

async fn get_instance_by_id(pool: &SqlitePool, id: Uuid) -> Result<Instance, AppError> {
    let row: Option<InstanceRow> =
        sqlx::query_as::<_, InstanceRow>(r#"SELECT * FROM instances WHERE id=? "#)
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;

    let Some(row) = row else {
        return Err(AppError::not_found("instance not found"));
    };

    // Parse enum-like fields before consuming the row.
    let cfg_mode = parse_config_mode_db(&row.config_mode);
    let restart = parse_restart_policy_db(&row.restart_policy);

    // runtime filled by caller
    let mut inst = row.to_instance(None)?;

    inst.config_mode = cfg_mode;
    inst.restart_policy = restart;

    Ok(inst)
}

fn validate_create(req: &InstanceCreateRequest) -> Result<(), AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::bad_request("name is required"));
    }
    if req.command.trim().is_empty() {
        return Err(AppError::bad_request("command is required"));
    }

    // Minimal cross-field validation:
    match req.config_mode {
        crate::models::ConfigMode::None => Ok(()),
        crate::models::ConfigMode::Path => {
            if req.config_path.as_deref().unwrap_or("").trim().is_empty() {
                Err(AppError::bad_request(
                    "config_path required when config_mode=path",
                ))
            } else {
                Ok(())
            }
        }
        crate::models::ConfigMode::Inline => {
            if req
                .config_filename
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err(AppError::bad_request(
                    "config_filename required when config_mode=inline",
                ));
            }
            if req.config_content.as_deref().unwrap_or("").is_empty() {
                return Err(AppError::bad_request(
                    "config_content required when config_mode=inline",
                ));
            }
            Ok(())
        }
    }
}

fn apply_patch(existing: &mut Instance, patch: InstanceUpdateRequest) -> Result<(), AppError> {
    if let Some(v) = patch.name {
        if v.trim().is_empty() {
            return Err(AppError::bad_request("name cannot be empty"));
        }
        existing.name = v;
    }
    if let Some(v) = patch.enabled {
        existing.enabled = v;
    }
    if let Some(v) = patch.command {
        if v.trim().is_empty() {
            return Err(AppError::bad_request("command cannot be empty"));
        }
        existing.command = v;
    }
    if let Some(v) = patch.args {
        existing.args = v;
    }
    if let Some(v) = patch.cwd {
        existing.cwd = v;
    }
    if let Some(v) = patch.env {
        existing.env = v;
    }
    if let Some(v) = patch.use_pty {
        existing.use_pty = v;
    }

    if let Some(v) = patch.config_mode {
        existing.config_mode = v;
    }
    if let Some(v) = patch.config_path {
        existing.config_path = v;
    }
    if let Some(v) = patch.config_filename {
        existing.config_filename = v;
    }
    if let Some(v) = patch.config_content {
        existing.config_content = v;
    }

    if let Some(v) = patch.restart_policy {
        existing.restart_policy = v;
    }
    if let Some(v) = patch.auto_start {
        existing.auto_start = v;
    }

    // If config_mode changes, validate consistency (basic)
    match existing.config_mode {
        crate::models::ConfigMode::None => Ok(()),
        crate::models::ConfigMode::Path => {
            if existing
                .config_path
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                Err(AppError::bad_request(
                    "config_path required when config_mode=path",
                ))
            } else {
                Ok(())
            }
        }
        crate::models::ConfigMode::Inline => {
            if existing
                .config_filename
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err(AppError::bad_request(
                    "config_filename required when config_mode=inline",
                ));
            }
            if existing.config_content.as_deref().unwrap_or("").is_empty() {
                return Err(AppError::bad_request(
                    "config_content required when config_mode=inline",
                ));
            }
            Ok(())
        }
    }
}

fn bool_to_i64(v: bool) -> i64 {
    if v {
        1
    } else {
        0
    }
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
        let root = unique_temp_dir("aicli-instances-route-test");
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
    async fn stop_instance_returns_not_found_for_missing_id() {
        let state = test_state().await;
        let id = Uuid::new_v4();

        let err = stop_instance(State(state), Path(id))
            .await
            .expect_err("missing instance should return not found");
        assert!(matches!(err, AppError::NotFound(_)));
    }
}
