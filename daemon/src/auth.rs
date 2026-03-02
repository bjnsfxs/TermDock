use crate::{error::AppError, models::now_rfc3339, state::AppState};
use axum::{
    body::Body,
    extract::State,
    http::{header, Request},
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub enum AuthPrincipal {
    Master,
    Device { device_id: String },
}

/// REST auth for regular APIs:
/// - requires Authorization header
/// - accepts master token OR active device token
pub async fn require_api_bearer_header(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    let token = token_from_authorization(req.headers().get(header::AUTHORIZATION))?;
    let principal = principal_from_token(&state, &token, true).await?;
    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}

/// REST auth for privileged APIs:
/// - requires Authorization header
/// - accepts master token only
pub async fn require_master_bearer_header(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    let token = token_from_authorization(req.headers().get(header::AUTHORIZATION))?;
    let principal = principal_from_token(&state, &token, false).await?;
    match principal {
        AuthPrincipal::Master => {
            req.extensions_mut().insert(AuthPrincipal::Master);
            Ok(next.run(req).await)
        }
        AuthPrincipal::Device { .. } => Err(AppError::Unauthorized),
    }
}

/// WS auth:
/// - Prefer `Authorization: Bearer <token>`
/// - Browser fallback `?token=<token>`
/// - accepts master token OR active device token
pub async fn require_ws_bearer_header_or_query(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    let token = if let Some(h) = req.headers().get(header::AUTHORIZATION) {
        token_from_authorization(Some(h))?
    } else {
        token_from_query(req.uri().query()).ok_or(AppError::Unauthorized)?
    };

    let principal = principal_from_token(&state, &token, true).await?;
    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}

pub fn token_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push(hex_digit((b >> 4) & 0x0f));
        out.push(hex_digit(b & 0x0f));
    }
    out
}

fn hex_digit(v: u8) -> char {
    match v {
        0..=9 => (b'0' + v) as char,
        10..=15 => (b'a' + (v - 10)) as char,
        _ => '0',
    }
}

async fn principal_from_token(
    state: &AppState,
    token: &str,
    update_device_last_seen: bool,
) -> Result<AuthPrincipal, AppError> {
    let cfg = state.config_read();
    if token == cfg.token {
        return Ok(AuthPrincipal::Master);
    }

    let hashed = token_hash(token);
    let device_id: Option<String> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM auth_devices
        WHERE token_hash = ?
          AND revoked_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(hashed)
    .fetch_optional(&state.db)
    .await?;

    let Some(device_id) = device_id else {
        return Err(AppError::Unauthorized);
    };

    if update_device_last_seen {
        let _ = sqlx::query(
            r#"
            UPDATE auth_devices
            SET last_seen_at = ?
            WHERE id = ?
            "#,
        )
        .bind(now_rfc3339())
        .bind(device_id.clone())
        .execute(&state.db)
        .await;
    }

    Ok(AuthPrincipal::Device { device_id })
}

fn token_from_authorization(
    header_value: Option<&axum::http::HeaderValue>,
) -> Result<String, AppError> {
    let Some(value) = header_value else {
        return Err(AppError::Unauthorized);
    };
    let Ok(s) = value.to_str() else {
        return Err(AppError::Unauthorized);
    };

    let token = s
        .strip_prefix("Bearer ")
        .or_else(|| s.strip_prefix("bearer "))
        .ok_or(AppError::Unauthorized)?;

    Ok(token.to_string())
}

fn token_from_query(query: Option<&str>) -> Option<String> {
    let q = query?;
    for pair in q.split('&') {
        let mut it = pair.splitn(2, '=');
        let k = percent_decode_component(it.next()?.trim())?;
        let v = percent_decode_component(it.next().unwrap_or("").trim())?;
        if k == "token" && !v.is_empty() {
            return Some(v);
        }
    }
    None
}

fn percent_decode_component(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                if i + 2 >= bytes.len() {
                    return None;
                }
                let hi = from_hex(bytes[i + 1])?;
                let lo = from_hex(bytes[i + 2])?;
                out.push((hi << 4) | lo);
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8(out).ok()
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + (b - b'a')),
        b'A'..=b'F' => Some(10 + (b - b'A')),
        _ => None,
    }
}
