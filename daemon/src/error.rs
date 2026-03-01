use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::borrow::Cow;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden: {0}")]
    Forbidden(Cow<'static, str>),

    #[error("not found: {0}")]
    NotFound(Cow<'static, str>),

    #[error("bad request: {0}")]
    BadRequest(Cow<'static, str>),

    #[error("conflict: {0}")]
    Conflict(Cow<'static, str>),

    #[error("internal: {0}")]
    Internal(Cow<'static, str>),

    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    #[error(transparent)]
    SqlxMigrate(#[from] sqlx::migrate::MigrateError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl AppError {
    pub fn not_found(msg: impl Into<Cow<'static, str>>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn forbidden(msg: impl Into<Cow<'static, str>>) -> Self {
        Self::Forbidden(msg.into())
    }

    pub fn bad_request(msg: impl Into<Cow<'static, str>>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn conflict(msg: impl Into<Cow<'static, str>>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn internal(msg: impl Into<Cow<'static, str>>) -> Self {
        Self::Internal(msg.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", self.to_string()),
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden", self.to_string()),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found", self.to_string()),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request", self.to_string()),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "conflict", self.to_string()),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                self.to_string(),
            ),
            AppError::Sqlx(e) => (StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string()),
            AppError::SqlxMigrate(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "migration_error",
                e.to_string(),
            ),
            AppError::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, "io_error", e.to_string()),
            AppError::Json(e) => (StatusCode::BAD_REQUEST, "json_error", e.to_string()),
        };

        let body = ErrorEnvelope {
            error: ErrorBody {
                code,
                message,
                details: None,
            },
        };

        (status, Json(body)).into_response()
    }
}
