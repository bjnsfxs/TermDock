use crate::{error::AppError, state::AppState};
use axum::{
    body::Body,
    extract::State,
    http::{header, Request},
    middleware::Next,
    response::Response,
};

/// REST auth: require `Authorization: Bearer <token>`.
pub async fn require_bearer_header(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    let token = token_from_authorization(req.headers().get(header::AUTHORIZATION))?;
    let expected = state.config_read().token;
    if token != expected {
        return Err(AppError::Unauthorized);
    }
    Ok(next.run(req).await)
}

/// WebSocket auth:
/// - Prefer `Authorization: Bearer <token>`
/// - Fallback to query param `?token=<token>` for browser clients (WebSocket API cannot set headers).
pub async fn require_bearer_header_or_query(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    let expected = state.config_read().token;

    if let Some(h) = req.headers().get(header::AUTHORIZATION) {
        let token = token_from_authorization(Some(h))?;
        if token != expected {
            return Err(AppError::Unauthorized);
        }
        return Ok(next.run(req).await);
    }

    // Fallback: query token
    let Some(token) = token_from_query(req.uri().query()) else {
        return Err(AppError::Unauthorized);
    };
    if token != expected {
        return Err(AppError::Unauthorized);
    }

    Ok(next.run(req).await)
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
