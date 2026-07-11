//! Request correlation and catch-all error normalization for the REST edge.

use axum::body::{to_bytes, Body};
use axum::extract::Request;
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;

use crate::error::ErrorEnvelope;

pub const X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

pub async fn correlate_and_normalize(request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(&X_REQUEST_ID)
        .and_then(|value| value.to_str().ok())
        .filter(|value| valid_request_id(value))
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let span = tracing::info_span!(
        "http_request",
        request_id = %request_id,
        method = %method,
        path = %path,
    );
    let mut response = next.run(request).instrument(span.clone()).await;
    let status = response.status();
    if status.is_client_error() || status.is_server_error() {
        response = normalize_error(response, &request_id).await;
    }
    response.headers_mut().insert(
        X_REQUEST_ID.clone(),
        HeaderValue::from_str(&request_id).expect("validated request id is a header value"),
    );
    span.in_scope(|| tracing::info!(status = status.as_u16(), "request completed"));
    response
}

fn valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

async fn normalize_error(response: Response, request_id: &str) -> Response {
    let status = response.status();
    let (mut parts, body) = response.into_parts();
    let bytes = to_bytes(body, MAX_ERROR_BODY_BYTES)
        .await
        .unwrap_or_default();
    let parsed = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
    let code = parsed
        .as_ref()
        .and_then(|value| value.get("code"))
        .and_then(|value| value.as_str())
        .unwrap_or_else(|| code_for_status(status));
    let message = parsed
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(|value| value.as_str())
        .or_else(|| {
            parsed
                .as_ref()
                .and_then(|value| value.get("error"))
                .and_then(|value| value.as_str())
        })
        .map(str::to_owned)
        .unwrap_or_else(|| safe_message_for_status(status).into());
    let details = parsed
        .as_ref()
        .and_then(|value| value.get("details"))
        .cloned();
    let retryable = parsed
        .as_ref()
        .and_then(|value| value.get("retryable"))
        .and_then(|value| value.as_bool())
        .unwrap_or_else(|| retryable_status(status));
    let envelope = ErrorEnvelope {
        version: crate::error::ERROR_ENVELOPE_VERSION.into(),
        code: code.into(),
        message,
        request_id: request_id.into(),
        retryable,
        details,
    };
    let encoded = serde_json::to_vec(&envelope).expect("error envelope serializes");
    parts.headers.remove(CONTENT_LENGTH);
    parts.headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    Response::from_parts(parts, Body::from(encoded))
}

fn code_for_status(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => "invalid_request",
        StatusCode::UNAUTHORIZED => "unauthenticated",
        StatusCode::FORBIDDEN => "permission_denied",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::METHOD_NOT_ALLOWED => "method_not_allowed",
        StatusCode::CONFLICT => "conflict",
        StatusCode::PAYLOAD_TOO_LARGE => "payload_too_large",
        StatusCode::RANGE_NOT_SATISFIABLE => "range_not_satisfiable",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        StatusCode::BAD_GATEWAY => "upstream_unavailable",
        StatusCode::SERVICE_UNAVAILABLE => "service_unavailable",
        StatusCode::GATEWAY_TIMEOUT => "upstream_timeout",
        _ => "internal_error",
    }
}

fn safe_message_for_status(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => "invalid request",
        StatusCode::UNAUTHORIZED => "authentication required",
        StatusCode::FORBIDDEN => "permission denied",
        StatusCode::NOT_FOUND => "resource not found",
        StatusCode::METHOD_NOT_ALLOWED => "method not allowed",
        StatusCode::CONFLICT => "request conflicts with current state or policy",
        StatusCode::PAYLOAD_TOO_LARGE => "request payload is too large",
        StatusCode::RANGE_NOT_SATISFIABLE => "requested range is not satisfiable",
        StatusCode::TOO_MANY_REQUESTS => "request rate limit exceeded",
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT => {
            "a required Clotho service is unavailable"
        }
        _ => "internal server error",
    }
}

fn retryable_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_ids_are_bounded_opaque_tokens() {
        assert!(valid_request_id("caller-123_abc.def"));
        assert!(!valid_request_id(""));
        assert!(!valid_request_id("contains a space"));
        assert!(!valid_request_id(&"a".repeat(129)));
    }

    #[test]
    fn status_codes_have_stable_classes() {
        assert_eq!(code_for_status(StatusCode::UNAUTHORIZED), "unauthenticated");
        assert_eq!(code_for_status(StatusCode::FORBIDDEN), "permission_denied");
        assert_eq!(code_for_status(StatusCode::CONFLICT), "conflict");
        assert_eq!(
            code_for_status(StatusCode::BAD_GATEWAY),
            "upstream_unavailable"
        );
        assert!(retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!retryable_status(StatusCode::BAD_REQUEST));
    }
}
