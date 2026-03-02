use axum::{
    extract::{ConnectInfo, Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use sqlx::FromRow;
use std::net::SocketAddr;
use uuid::Uuid;

use crate::{
    auth::token_hash,
    error::AppError,
    models::{
        now_rfc3339, now_unix_seconds, AuthDevice, AuthDeviceListResponse, PairCompleteRequest,
        PairCompleteResponse, PairDecision, PairDecisionRequest, PairDecisionResponse,
        PairStartRequest, PairStartResponse, PairStatusResponse, PendingPairSession,
        PendingPairSessionsResponse,
    },
    state::AppState,
};

const PAIR_STATUS_PENDING: &str = "pending";
const PAIR_STATUS_AWAITING_APPROVAL: &str = "awaiting-approval";
const PAIR_STATUS_APPROVED: &str = "approved";
const PAIR_STATUS_REJECTED: &str = "rejected";
const PAIR_STATUS_EXPIRED: &str = "expired";
const PAIR_STATUS_TOKEN_DELIVERED: &str = "token-delivered";

const DEFAULT_PAIR_TTL_SECONDS: u64 = 120;
const MIN_PAIR_TTL_SECONDS: u64 = 30;
const MAX_PAIR_TTL_SECONDS: u64 = 600;

#[derive(Debug, Deserialize)]
pub struct PairStatusQuery {
    pub secret: String,
}

#[derive(Debug, Clone, FromRow)]
struct PairSessionRow {
    id: String,
    pair_secret_hash: String,
    status: String,
    requested_name: Option<String>,
    platform: Option<String>,
    expires_at_epoch: i64,
    issued_device_id: Option<String>,
    issued_token: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone, FromRow)]
struct AuthDeviceRow {
    id: String,
    name: String,
    platform: Option<String>,
    created_at: String,
    last_seen_at: String,
    revoked_at: Option<String>,
}

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/auth/pair/complete", post(complete_pair))
        .route("/auth/pair/status/:pair_id", get(pair_status))
}

pub fn master_router() -> Router<AppState> {
    Router::new()
        .route("/auth/pair/start", post(start_pair))
        .route("/auth/pair/pending", get(list_pending_pairs))
        .route("/auth/pair/decision", post(pair_decision))
        .route("/auth/devices", get(list_devices))
        .route("/auth/devices/:device_id", delete(revoke_device))
}

pub async fn start_pair(
    State(state): State<AppState>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    Json(req): Json<PairStartRequest>,
) -> Result<Json<PairStartResponse>, AppError> {
    if !remote_addr.ip().is_loopback() {
        return Err(AppError::forbidden(
            "pair start can only be requested from loopback clients",
        ));
    }

    let ttl = req
        .ttl_seconds
        .unwrap_or(DEFAULT_PAIR_TTL_SECONDS)
        .clamp(MIN_PAIR_TTL_SECONDS, MAX_PAIR_TTL_SECONDS);
    let now = now_rfc3339();
    let now_epoch = now_unix_seconds();
    let expires_at_epoch = now_epoch + ttl as i64;

    let pair_id = Uuid::new_v4().to_string();
    let pair_secret = Uuid::new_v4().to_string().replace('-', "");
    let pair_secret_hash = token_hash(&pair_secret);

    sqlx::query(
        r#"
        INSERT INTO pair_sessions (
          id,
          pair_secret_hash,
          status,
          requested_name,
          platform,
          expires_at_epoch,
          issued_device_id,
          issued_token,
          approved_at,
          rejected_at,
          delivered_at,
          created_at,
          updated_at
        )
        VALUES (?, ?, ?, NULL, NULL, ?, NULL, NULL, NULL, NULL, NULL, ?, ?)
        "#,
    )
    .bind(pair_id.clone())
    .bind(pair_secret_hash)
    .bind(PAIR_STATUS_PENDING)
    .bind(expires_at_epoch)
    .bind(now.clone())
    .bind(now)
    .execute(&state.db)
    .await?;

    let cfg = state.config_read();
    let default_base_url = format!("http://{}:{}", cfg.bind_address, cfg.port);
    let base_url = req
        .base_url
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or(default_base_url);

    let pair_uri = format!(
        "aicli-manager://pair?baseUrl={}&pairId={}&pairSecret={}",
        percent_encode_component(&base_url),
        percent_encode_component(&pair_id),
        percent_encode_component(&pair_secret),
    );

    Ok(Json(PairStartResponse {
        pair_id,
        pair_secret,
        pair_uri,
        expires_at_epoch,
        expires_in_seconds: ttl,
    }))
}

pub async fn complete_pair(
    State(state): State<AppState>,
    Json(req): Json<PairCompleteRequest>,
) -> Result<Json<PairCompleteResponse>, AppError> {
    let pair_id = req.pair_id.trim();
    if pair_id.is_empty() {
        return Err(AppError::bad_request("pair_id is required"));
    }

    let pair_secret = req.pair_secret.trim();
    if pair_secret.is_empty() {
        return Err(AppError::bad_request("pair_secret is required"));
    }

    let device_name = req.device_name.trim();
    if device_name.is_empty() {
        return Err(AppError::bad_request("device_name is required"));
    }

    let mut session = get_pair_session(&state, pair_id).await?;
    ensure_pair_secret(&session, pair_secret)?;
    maybe_expire_session(&state, &mut session).await?;

    if session.status == PAIR_STATUS_REJECTED {
        return Err(AppError::conflict("pair session rejected"));
    }
    if session.status == PAIR_STATUS_EXPIRED {
        return Err(AppError::conflict("pair session expired"));
    }
    if session.status == PAIR_STATUS_APPROVED || session.status == PAIR_STATUS_TOKEN_DELIVERED {
        return Err(AppError::conflict("pair session already completed"));
    }

    let updated_at = now_rfc3339();
    sqlx::query(
        r#"
        UPDATE pair_sessions
        SET status = ?, requested_name = ?, platform = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(PAIR_STATUS_AWAITING_APPROVAL)
    .bind(device_name)
    .bind(
        req.platform
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
    )
    .bind(updated_at)
    .bind(pair_id)
    .execute(&state.db)
    .await?;

    Ok(Json(PairCompleteResponse {
        status: "pending-approval".to_string(),
    }))
}

pub async fn pair_status(
    State(state): State<AppState>,
    Path(pair_id): Path<String>,
    Query(q): Query<PairStatusQuery>,
) -> Result<Json<PairStatusResponse>, AppError> {
    if q.secret.trim().is_empty() {
        return Err(AppError::bad_request("secret is required"));
    }

    let mut session = get_pair_session(&state, pair_id.trim()).await?;
    ensure_pair_secret(&session, q.secret.trim())?;
    maybe_expire_session(&state, &mut session).await?;

    match session.status.as_str() {
        PAIR_STATUS_PENDING | PAIR_STATUS_AWAITING_APPROVAL => Ok(Json(PairStatusResponse {
            status: "pending-approval".to_string(),
            device_id: None,
            device_token: None,
            message: None,
        })),
        PAIR_STATUS_REJECTED => Ok(Json(PairStatusResponse {
            status: "rejected".to_string(),
            device_id: None,
            device_token: None,
            message: Some("pair request rejected".to_string()),
        })),
        PAIR_STATUS_EXPIRED => Ok(Json(PairStatusResponse {
            status: "expired".to_string(),
            device_id: None,
            device_token: None,
            message: Some("pair request expired".to_string()),
        })),
        PAIR_STATUS_APPROVED => {
            let token = session.issued_token.clone().ok_or_else(|| {
                AppError::internal("approved pair session is missing issued token")
            })?;

            if claim_approved_pair_token(&state, &session.id, &token).await? {
                Ok(Json(PairStatusResponse {
                    status: "approved".to_string(),
                    device_id: session.issued_device_id,
                    device_token: Some(token),
                    message: None,
                }))
            } else {
                Ok(Json(
                    resolve_failed_approved_claim(&state, &session.id).await?,
                ))
            }
        }
        PAIR_STATUS_TOKEN_DELIVERED => Ok(Json(token_delivered_response(session.issued_device_id))),
        _ => Ok(Json(PairStatusResponse {
            status: session.status,
            device_id: session.issued_device_id,
            device_token: None,
            message: None,
        })),
    }
}

pub async fn list_pending_pairs(
    State(state): State<AppState>,
) -> Result<Json<PendingPairSessionsResponse>, AppError> {
    expire_pending_sessions(&state).await?;
    let now = now_unix_seconds();

    let rows: Vec<PairSessionRow> = sqlx::query_as::<_, PairSessionRow>(
        r#"
        SELECT
          id,
          pair_secret_hash,
          status,
          requested_name,
          platform,
          expires_at_epoch,
          issued_device_id,
          issued_token,
          created_at
        FROM pair_sessions
        WHERE status = ?
          AND expires_at_epoch > ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(PAIR_STATUS_AWAITING_APPROVAL)
    .bind(now)
    .fetch_all(&state.db)
    .await?;

    let sessions = rows
        .into_iter()
        .map(|row| PendingPairSession {
            pair_id: row.id,
            requested_name: row.requested_name,
            platform: row.platform,
            created_at: row.created_at,
            expires_at_epoch: row.expires_at_epoch,
        })
        .collect();

    Ok(Json(PendingPairSessionsResponse { sessions }))
}

pub async fn pair_decision(
    State(state): State<AppState>,
    Json(req): Json<PairDecisionRequest>,
) -> Result<Json<PairDecisionResponse>, AppError> {
    let pair_id = req.pair_id.trim();
    if pair_id.is_empty() {
        return Err(AppError::bad_request("pair_id is required"));
    }

    let mut session = get_pair_session(&state, pair_id).await?;
    maybe_expire_session(&state, &mut session).await?;

    match req.decision {
        PairDecision::Approve => {
            if session.status != PAIR_STATUS_AWAITING_APPROVAL {
                return Err(AppError::conflict(
                    "pair session is not waiting for approval",
                ));
            }

            let device_id = Uuid::new_v4().to_string();
            let device_token = Uuid::new_v4().to_string();
            let token_hash_value = token_hash(&device_token);
            let now = now_rfc3339();

            sqlx::query(
                r#"
                INSERT INTO auth_devices (
                  id,
                  name,
                  platform,
                  token_hash,
                  created_at,
                  last_seen_at,
                  revoked_at
                )
                VALUES (?, ?, ?, ?, ?, ?, NULL)
                "#,
            )
            .bind(&device_id)
            .bind(
                session
                    .requested_name
                    .clone()
                    .unwrap_or_else(|| "mobile-device".to_string()),
            )
            .bind(session.platform.clone())
            .bind(token_hash_value)
            .bind(now.clone())
            .bind(now.clone())
            .execute(&state.db)
            .await?;

            sqlx::query(
                r#"
                UPDATE pair_sessions
                SET status = ?, issued_device_id = ?, issued_token = ?, approved_at = ?, updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(PAIR_STATUS_APPROVED)
            .bind(&device_id)
            .bind(device_token)
            .bind(now.clone())
            .bind(now)
            .bind(pair_id)
            .execute(&state.db)
            .await?;

            Ok(Json(PairDecisionResponse {
                status: "approved".to_string(),
                device_id: Some(device_id),
            }))
        }
        PairDecision::Reject => {
            if session.status != PAIR_STATUS_AWAITING_APPROVAL
                && session.status != PAIR_STATUS_PENDING
            {
                return Err(AppError::conflict(
                    "pair session is not waiting for approval",
                ));
            }
            let now = now_rfc3339();
            sqlx::query(
                r#"
                UPDATE pair_sessions
                SET status = ?, rejected_at = ?, updated_at = ?, issued_token = NULL
                WHERE id = ?
                "#,
            )
            .bind(PAIR_STATUS_REJECTED)
            .bind(now.clone())
            .bind(now)
            .bind(pair_id)
            .execute(&state.db)
            .await?;

            Ok(Json(PairDecisionResponse {
                status: "rejected".to_string(),
                device_id: None,
            }))
        }
    }
}

pub async fn list_devices(
    State(state): State<AppState>,
) -> Result<Json<AuthDeviceListResponse>, AppError> {
    let rows: Vec<AuthDeviceRow> = sqlx::query_as::<_, AuthDeviceRow>(
        r#"
        SELECT id, name, platform, created_at, last_seen_at, revoked_at
        FROM auth_devices
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(AuthDeviceListResponse {
        devices: rows
            .into_iter()
            .map(|row| AuthDevice {
                id: row.id,
                name: row.name,
                platform: row.platform,
                created_at: row.created_at,
                last_seen_at: row.last_seen_at,
                revoked_at: row.revoked_at,
            })
            .collect(),
    }))
}

pub async fn revoke_device(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<axum::http::StatusCode, AppError> {
    let now = now_rfc3339();
    let res = sqlx::query(
        r#"
        UPDATE auth_devices
        SET revoked_at = ?
        WHERE id = ?
          AND revoked_at IS NULL
        "#,
    )
    .bind(now)
    .bind(device_id.trim())
    .execute(&state.db)
    .await?;

    if res.rows_affected() == 0 {
        return Err(AppError::not_found("device not found"));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn get_pair_session(state: &AppState, pair_id: &str) -> Result<PairSessionRow, AppError> {
    let row: Option<PairSessionRow> = sqlx::query_as::<_, PairSessionRow>(
        r#"
        SELECT
          id,
          pair_secret_hash,
          status,
          requested_name,
          platform,
          expires_at_epoch,
          issued_device_id,
          issued_token,
          created_at
        FROM pair_sessions
        WHERE id = ?
        LIMIT 1
        "#,
    )
    .bind(pair_id)
    .fetch_optional(&state.db)
    .await?;

    row.ok_or_else(|| AppError::not_found("pair session not found"))
}

fn ensure_pair_secret(row: &PairSessionRow, secret: &str) -> Result<(), AppError> {
    if row.pair_secret_hash == token_hash(secret) {
        Ok(())
    } else {
        Err(AppError::not_found("pair session not found"))
    }
}

async fn maybe_expire_session(state: &AppState, row: &mut PairSessionRow) -> Result<(), AppError> {
    let now = now_unix_seconds();
    if row.expires_at_epoch <= now
        && (row.status == PAIR_STATUS_PENDING || row.status == PAIR_STATUS_AWAITING_APPROVAL)
    {
        let updated_at = now_rfc3339();
        sqlx::query(
            r#"
            UPDATE pair_sessions
            SET status = ?, issued_token = NULL, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(PAIR_STATUS_EXPIRED)
        .bind(updated_at)
        .bind(&row.id)
        .execute(&state.db)
        .await?;
        row.status = PAIR_STATUS_EXPIRED.to_string();
    }
    Ok(())
}

async fn claim_approved_pair_token(
    state: &AppState,
    pair_id: &str,
    expected_token: &str,
) -> Result<bool, AppError> {
    let now = now_rfc3339();
    let res = sqlx::query(
        r#"
        UPDATE pair_sessions
        SET status = ?, issued_token = NULL, delivered_at = ?, updated_at = ?
        WHERE id = ?
          AND status = ?
          AND issued_token = ?
        "#,
    )
    .bind(PAIR_STATUS_TOKEN_DELIVERED)
    .bind(now.clone())
    .bind(now)
    .bind(pair_id)
    .bind(PAIR_STATUS_APPROVED)
    .bind(expected_token)
    .execute(&state.db)
    .await?;
    Ok(res.rows_affected() == 1)
}

async fn resolve_failed_approved_claim(
    state: &AppState,
    pair_id: &str,
) -> Result<PairStatusResponse, AppError> {
    let latest = get_pair_session(state, pair_id).await?;
    match latest.status.as_str() {
        PAIR_STATUS_TOKEN_DELIVERED => Ok(token_delivered_response(latest.issued_device_id)),
        PAIR_STATUS_APPROVED => {
            let latest_token = latest.issued_token.clone().ok_or_else(|| {
                AppError::internal("approved pair session is missing issued token")
            })?;

            if claim_approved_pair_token(state, &latest.id, &latest_token).await? {
                Ok(PairStatusResponse {
                    status: "approved".to_string(),
                    device_id: latest.issued_device_id,
                    device_token: Some(latest_token),
                    message: None,
                })
            } else {
                let settled = get_pair_session(state, &latest.id).await?;
                match settled.status.as_str() {
                    PAIR_STATUS_TOKEN_DELIVERED => {
                        Ok(token_delivered_response(settled.issued_device_id))
                    }
                    PAIR_STATUS_APPROVED => Ok(PairStatusResponse {
                        status: "pending-approval".to_string(),
                        device_id: settled.issued_device_id,
                        device_token: None,
                        message: Some("pair approval is settling; retry status poll".to_string()),
                    }),
                    _ => Ok(PairStatusResponse {
                        status: settled.status,
                        device_id: settled.issued_device_id,
                        device_token: None,
                        message: None,
                    }),
                }
            }
        }
        _ => Ok(PairStatusResponse {
            status: latest.status,
            device_id: latest.issued_device_id,
            device_token: None,
            message: None,
        }),
    }
}

fn token_delivered_response(device_id: Option<String>) -> PairStatusResponse {
    PairStatusResponse {
        status: "token-delivered".to_string(),
        device_id,
        device_token: None,
        message: Some("token already delivered".to_string()),
    }
}

async fn expire_pending_sessions(state: &AppState) -> Result<(), AppError> {
    let now = now_unix_seconds();
    let updated_at = now_rfc3339();
    sqlx::query(
        r#"
        UPDATE pair_sessions
        SET status = ?, issued_token = NULL, updated_at = ?
        WHERE expires_at_epoch <= ?
          AND (status = ? OR status = ?)
        "#,
    )
    .bind(PAIR_STATUS_EXPIRED)
    .bind(updated_at)
    .bind(now)
    .bind(PAIR_STATUS_PENDING)
    .bind(PAIR_STATUS_AWAITING_APPROVAL)
    .execute(&state.db)
    .await?;
    Ok(())
}

fn percent_encode_component(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if is_unreserved_uri_byte(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex_digit((b >> 4) & 0x0f));
            out.push(hex_digit(b & 0x0f));
        }
    }
    out
}

fn is_unreserved_uri_byte(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~')
}

fn hex_digit(v: u8) -> char {
    match v {
        0..=9 => (b'0' + v) as char,
        10..=15 => (b'A' + (v - 10)) as char,
        _ => '0',
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
        let root = unique_temp_dir("aicli-pairing-route-test");
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
    async fn pair_roundtrip_approve_and_deliver_once() {
        let state = test_state().await;
        let addr: SocketAddr = "127.0.0.1:9876".parse().expect("parse socket addr");

        let start = start_pair(
            State(state.clone()),
            ConnectInfo(addr),
            Json(PairStartRequest {
                base_url: Some("http://192.168.1.8:8765".to_string()),
                ttl_seconds: Some(120),
            }),
        )
        .await
        .expect("pair start should succeed")
        .0;

        let _ = complete_pair(
            State(state.clone()),
            Json(PairCompleteRequest {
                pair_id: start.pair_id.clone(),
                pair_secret: start.pair_secret.clone(),
                device_name: "pixel".to_string(),
                platform: Some("android".to_string()),
            }),
        )
        .await
        .expect("pair complete should succeed");

        let pending = list_pending_pairs(State(state.clone()))
            .await
            .expect("pending pairs should load")
            .0;
        assert_eq!(pending.sessions.len(), 1);
        assert_eq!(pending.sessions[0].pair_id, start.pair_id);

        let _ = pair_decision(
            State(state.clone()),
            Json(PairDecisionRequest {
                pair_id: start.pair_id.clone(),
                decision: PairDecision::Approve,
            }),
        )
        .await
        .expect("pair approve should succeed");

        let first_status = pair_status(
            State(state.clone()),
            Path(start.pair_id.clone()),
            Query(PairStatusQuery {
                secret: start.pair_secret.clone(),
            }),
        )
        .await
        .expect("status should load")
        .0;
        assert_eq!(first_status.status, "approved");
        assert!(first_status.device_token.is_some());

        let second_status = pair_status(
            State(state),
            Path(start.pair_id),
            Query(PairStatusQuery {
                secret: start.pair_secret,
            }),
        )
        .await
        .expect("second status should load")
        .0;
        assert_eq!(second_status.status, "token-delivered");
        assert!(second_status.device_token.is_none());
    }

    #[tokio::test]
    async fn approved_token_claim_is_atomic_for_stale_readers() {
        let state = test_state().await;
        let addr: SocketAddr = "127.0.0.1:9877".parse().expect("parse socket addr");

        let start = start_pair(
            State(state.clone()),
            ConnectInfo(addr),
            Json(PairStartRequest {
                base_url: Some("http://192.168.1.9:8765".to_string()),
                ttl_seconds: Some(120),
            }),
        )
        .await
        .expect("pair start should succeed")
        .0;

        let _ = complete_pair(
            State(state.clone()),
            Json(PairCompleteRequest {
                pair_id: start.pair_id.clone(),
                pair_secret: start.pair_secret.clone(),
                device_name: "pixel".to_string(),
                platform: Some("android".to_string()),
            }),
        )
        .await
        .expect("pair complete should succeed");

        let _ = pair_decision(
            State(state.clone()),
            Json(PairDecisionRequest {
                pair_id: start.pair_id.clone(),
                decision: PairDecision::Approve,
            }),
        )
        .await
        .expect("pair approve should succeed");

        let stale_reader_a = get_pair_session(&state, &start.pair_id)
            .await
            .expect("session should exist");
        let stale_reader_b = get_pair_session(&state, &start.pair_id)
            .await
            .expect("session should exist");
        let token_a = stale_reader_a
            .issued_token
            .clone()
            .expect("approved session should have token");
        let token_b = stale_reader_b
            .issued_token
            .clone()
            .expect("approved session should have token");

        let first_claim = claim_approved_pair_token(&state, &stale_reader_a.id, &token_a)
            .await
            .expect("first claim should run");
        let second_claim = claim_approved_pair_token(&state, &stale_reader_b.id, &token_b)
            .await
            .expect("second claim should run");

        assert!(first_claim, "first stale reader should claim token");
        assert!(
            !second_claim,
            "second stale reader must not claim token again"
        );

        let final_session = get_pair_session(&state, &start.pair_id)
            .await
            .expect("final session should exist");
        assert_eq!(final_session.status, PAIR_STATUS_TOKEN_DELIVERED);
        assert!(final_session.issued_token.is_none());
    }

    #[tokio::test]
    async fn failed_claim_on_stale_token_retries_latest_instead_of_reporting_delivered() {
        let state = test_state().await;
        let addr: SocketAddr = "127.0.0.1:9878".parse().expect("parse socket addr");

        let start = start_pair(
            State(state.clone()),
            ConnectInfo(addr),
            Json(PairStartRequest {
                base_url: Some("http://192.168.1.10:8765".to_string()),
                ttl_seconds: Some(120),
            }),
        )
        .await
        .expect("pair start should succeed")
        .0;

        let _ = complete_pair(
            State(state.clone()),
            Json(PairCompleteRequest {
                pair_id: start.pair_id.clone(),
                pair_secret: start.pair_secret.clone(),
                device_name: "pixel".to_string(),
                platform: Some("android".to_string()),
            }),
        )
        .await
        .expect("pair complete should succeed");

        let _ = pair_decision(
            State(state.clone()),
            Json(PairDecisionRequest {
                pair_id: start.pair_id.clone(),
                decision: PairDecision::Approve,
            }),
        )
        .await
        .expect("pair approve should succeed");

        let stale = get_pair_session(&state, &start.pair_id)
            .await
            .expect("session should exist");
        let stale_token = stale
            .issued_token
            .clone()
            .expect("approved session should have token");
        let rotated_token = format!("rotated-{}", Uuid::new_v4());
        let now = now_rfc3339();

        sqlx::query(
            r#"
            UPDATE pair_sessions
            SET issued_token = ?, updated_at = ?
            WHERE id = ?
              AND status = ?
            "#,
        )
        .bind(&rotated_token)
        .bind(now)
        .bind(&stale.id)
        .bind(PAIR_STATUS_APPROVED)
        .execute(&state.db)
        .await
        .expect("token rotation update should succeed");

        let stale_claim = claim_approved_pair_token(&state, &stale.id, &stale_token)
            .await
            .expect("stale claim should run");
        assert!(!stale_claim, "stale token claim must fail");

        let resolved = resolve_failed_approved_claim(&state, &stale.id)
            .await
            .expect("failed claim fallback should resolve");
        assert_eq!(resolved.status, "approved");
        assert_eq!(
            resolved.device_token.as_deref(),
            Some(rotated_token.as_str())
        );

        let final_session = get_pair_session(&state, &start.pair_id)
            .await
            .expect("final session should exist");
        assert_eq!(final_session.status, PAIR_STATUS_TOKEN_DELIVERED);
        assert!(final_session.issued_token.is_none());
    }
}
