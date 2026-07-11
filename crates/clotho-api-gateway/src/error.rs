//! Stable edge errors. Request middleware adds the request id to every error
//! and propagates it in `X-Request-Id`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

pub const ERROR_ENVELOPE_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    pub version: String,
    pub code: String,
    pub message: String,
    pub request_id: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorEnvelope {
    pub fn new(code: &str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            version: ERROR_ENVELOPE_VERSION.into(),
            code: code.into(),
            message: message.into(),
            request_id: String::new(),
            retryable,
            details: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{message}")]
    RangeNotSatisfiable { message: String, size: u64 },
    /// A backing service (clotho-vcs, Forgejo) failed or is unreachable.
    #[error("{0}")]
    Upstream(String),
    #[error("{0}")]
    Internal(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
}

impl ApiError {
    pub(crate) fn status(&self) -> StatusCode {
        match self {
            Self::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::RangeNotSatisfiable { .. } => StatusCode::RANGE_NOT_SATISFIABLE,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
        }
    }

    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "invalid_request",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::RangeNotSatisfiable { .. } => "range_not_satisfiable",
            Self::Upstream(_) => "upstream_unavailable",
            Self::Internal(_) => "internal_error",
            Self::Unauthorized(_) => "unauthenticated",
            Self::Forbidden(_) => "permission_denied",
        }
    }

    pub(crate) fn safe_message(&self) -> String {
        match self {
            Self::Upstream(_) => "a required Clotho service is unavailable".into(),
            Self::Internal(_) => "internal server error".into(),
            _ => self.to_string(),
        }
    }

    pub(crate) fn retryable(&self) -> bool {
        matches!(self, Self::Upstream(_))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let content_range = match &self {
            Self::RangeNotSatisfiable { size, .. } => Some(format!("bytes */{size}")),
            _ => None,
        };
        let status = self.status();
        let envelope = ErrorEnvelope::new(self.code(), self.safe_message(), self.retryable());
        if status.is_server_error() {
            tracing::error!(error = %self, code = self.code(), "gateway request failed");
        } else {
            tracing::warn!(error = %self, code = self.code(), "gateway request rejected");
        }
        let body = Json(envelope);
        let mut response = (status, body).into_response();
        if let Some(value) = content_range.and_then(|value| value.parse().ok()) {
            response
                .headers_mut()
                .insert(axum::http::header::CONTENT_RANGE, value);
        }
        response
    }
}

/// Map a clotho-vcs gRPC error onto the edge, keeping the caller-facing
/// distinction between bad requests and upstream failures.
impl From<tonic::Status> for ApiError {
    fn from(status: tonic::Status) -> Self {
        match status.code() {
            tonic::Code::InvalidArgument => Self::InvalidRequest(status.message().to_string()),
            tonic::Code::NotFound => Self::NotFound("resource not found".into()),
            tonic::Code::AlreadyExists => Self::Conflict(status.message().to_string()),
            _ => Self::Upstream(format!("Clotho internal service: {}", status.message())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_codes_and_safe_server_messages() {
        let cases = [
            (
                ApiError::InvalidRequest("bad field".into()),
                "invalid_request",
                false,
            ),
            (ApiError::NotFound("missing".into()), "not_found", false),
            (ApiError::Conflict("blocked".into()), "conflict", false),
            (
                ApiError::Unauthorized("token expired".into()),
                "unauthenticated",
                false,
            ),
            (
                ApiError::Forbidden("no access".into()),
                "permission_denied",
                false,
            ),
            (
                ApiError::Upstream("forgejo at internal:3000 failed".into()),
                "upstream_unavailable",
                true,
            ),
        ];
        for (error, code, retryable) in cases {
            assert_eq!(error.code(), code);
            assert_eq!(error.retryable(), retryable);
        }
        assert_eq!(
            ApiError::Internal("database password leaked".into()).safe_message(),
            "internal server error"
        );
        assert_eq!(
            ApiError::Upstream("internal topology".into()).safe_message(),
            "a required Clotho service is unavailable"
        );
    }
}
