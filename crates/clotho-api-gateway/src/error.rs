//! Edge error type: every failure maps to an HTTP status + JSON body.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

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
    fn status(&self) -> StatusCode {
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
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let content_range = match &self {
            Self::RangeNotSatisfiable { size, .. } => Some(format!("bytes */{size}")),
            _ => None,
        };
        let body = Json(serde_json::json!({ "error": self.to_string() }));
        let mut response = (self.status(), body).into_response();
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
        let msg = format!("Clotho internal service: {}", status.message());
        match status.code() {
            tonic::Code::InvalidArgument => Self::InvalidRequest(msg),
            tonic::Code::NotFound => Self::NotFound(msg),
            tonic::Code::AlreadyExists => Self::Conflict(msg),
            _ => Self::Upstream(msg),
        }
    }
}
