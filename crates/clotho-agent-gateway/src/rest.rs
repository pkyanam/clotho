//! Thin HTTP client for the api-gateway REST edge.
//!
//! Stage 15 MCP collab/Actions/platform tools call this surface so agents
//! cannot drift from the public product contract (docs/openapi.yaml).

use std::future::Future;

use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::Value;

use crate::identity::ForwardedAgentBearer;
use crate::mcp::ToolError;

tokio::task_local! {
    /// Credential for the one authorized MCP tool future currently being
    /// polled. Task-local scope prevents credentials from becoming global
    /// client state or crossing concurrent agent requests.
    static FORWARDED_AGENT_BEARER: ForwardedAgentBearer;
}

pub(crate) async fn with_forwarded_agent_bearer<F>(
    bearer: ForwardedAgentBearer,
    future: F,
) -> F::Output
where
    F: Future,
{
    FORWARDED_AGENT_BEARER.scope(bearer, future).await
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    #[serde(default = "default_error_code")]
    code: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    request_id: String,
    #[serde(default)]
    retryable: bool,
    #[serde(default)]
    details: Option<Value>,
}

fn default_error_code() -> String {
    "http_error".into()
}

#[derive(Clone)]
pub struct RestClient {
    base_url: String,
    http: reqwest::Client,
}

impl RestClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn get(&self, path: &str) -> Result<Value, ToolError> {
        self.request(reqwest::Method::GET, path, None, &[]).await
    }

    pub async fn post(&self, path: &str, body: Value) -> Result<Value, ToolError> {
        self.request(reqwest::Method::POST, path, Some(body), &[])
            .await
    }

    pub async fn post_with_idempotency_key(
        &self,
        path: &str,
        body: Value,
        idempotency_key: Option<&str>,
    ) -> Result<Value, ToolError> {
        let headers = idempotency_key
            .map(|key| vec![("Idempotency-Key", key)])
            .unwrap_or_default();
        self.request(reqwest::Method::POST, path, Some(body), &headers)
            .await
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
        headers: &[(&str, &str)],
    ) -> Result<Value, ToolError> {
        let url = format!("{}{path}", self.base_url);
        let bearer = FORWARDED_AGENT_BEARER.try_with(Clone::clone).map_err(|_| {
            ToolError::Mcp(rmcp::ErrorData::internal_error(
                "REST request reached the client without an authenticated agent credential",
                None,
            ))
        })?;
        let mut req = self
            .http
            .request(method.clone(), &url)
            .bearer_auth(bearer.expose());
        if let Some(body) = body {
            req = req.json(&body);
        }
        for (name, value) in headers {
            req = req.header(*name, *value);
        }
        let response = req.send().await.map_err(|e| {
            ToolError::Mcp(rmcp::ErrorData::internal_error(
                format!("api-gateway request failed: {e}"),
                None,
            ))
        })?;
        let status = response.status();
        let text = response.text().await.map_err(|e| {
            ToolError::Mcp(rmcp::ErrorData::internal_error(
                format!("api-gateway read body failed: {e}"),
                None,
            ))
        })?;

        if status == StatusCode::NO_CONTENT || text.is_empty() {
            if status.is_success() {
                return Ok(Value::Null);
            }
            return Err(map_status(
                status,
                ErrorEnvelope {
                    code: default_error_code(),
                    message: format!("{method} {path} failed"),
                    request_id: String::new(),
                    retryable: false,
                    details: None,
                },
            ));
        }

        let value: Value =
            serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text.clone()));

        if !status.is_success() {
            let envelope =
                serde_json::from_value::<ErrorEnvelope>(value.clone()).unwrap_or(ErrorEnvelope {
                    code: default_error_code(),
                    message: value
                        .get("error")
                        .and_then(|error| error.as_str())
                        .unwrap_or(&text)
                        .to_owned(),
                    request_id: String::new(),
                    retryable: false,
                    details: None,
                });
            return Err(map_status(status, envelope));
        }
        Ok(value)
    }
}

fn map_status(status: StatusCode, envelope: ErrorEnvelope) -> ToolError {
    use rmcp::ErrorData as McpError;
    let data = Some(serde_json::json!({
        "version": "1",
        "code": envelope.code,
        "request_id": envelope.request_id,
        "retryable": envelope.retryable,
        "details": envelope.details,
        "http_status": status.as_u16(),
    }));
    let err = if status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND {
        McpError::invalid_params(envelope.message, data)
    } else {
        McpError::internal_error(envelope.message, data)
    };
    ToolError::Mcp(err)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::extract::{Path, State};
    use axum::http::HeaderMap;
    use axum::routing::get;
    use axum::{Json, Router};
    use serde_json::json;

    use super::*;

    #[derive(Clone)]
    struct ExpectedAuthorization {
        first_hash: Vec<u8>,
        second_hash: Vec<u8>,
    }

    async fn authorization_matches(
        State(expected): State<Arc<ExpectedAuthorization>>,
        Path(slot): Path<String>,
        headers: HeaderMap,
    ) -> Json<Value> {
        let actual = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let expected_hash = match slot.as_str() {
            "first" => &expected.first_hash,
            "second" => &expected.second_hash,
            _ => return Json(json!({ "matched": false })),
        };
        Json(json!({
            "matched": crate::identity::sha256(actual.as_bytes()) == *expected_hash,
        }))
    }

    async fn test_server(first: &str, second: &str) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new(ExpectedAuthorization {
            first_hash: crate::identity::sha256(format!("Bearer {first}").as_bytes()),
            second_hash: crate::identity::sha256(format!("Bearer {second}").as_bytes()),
        });
        let handle = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/authorization/{slot}", get(authorization_matches))
                    .with_state(state),
            )
            .await
            .unwrap();
        });
        (format!("http://{address}"), handle)
    }

    #[tokio::test]
    async fn forwards_the_scoped_agent_bearer_without_cross_request_bleed() {
        let first = "clotho_agt_first-test-credential".to_string();
        let second = "clotho_agt_second-test-credential".to_string();
        let (base_url, server) = test_server(&first, &second).await;
        let client = RestClient::new(base_url);

        let (first_result, second_result) = tokio::join!(
            with_forwarded_agent_bearer(
                ForwardedAgentBearer::new(first.clone()),
                client.get("/authorization/first"),
            ),
            with_forwarded_agent_bearer(
                ForwardedAgentBearer::new(second.clone()),
                client.get("/authorization/second"),
            ),
        );

        let first_result = match first_result {
            Ok(value) => value,
            Err(_) => panic!("first forwarded REST request failed"),
        };
        let second_result = match second_result {
            Ok(value) => value,
            Err(_) => panic!("second forwarded REST request failed"),
        };
        assert_eq!(first_result["matched"], true);
        assert_eq!(second_result["matched"], true);
        server.abort();
    }

    #[tokio::test]
    async fn refuses_to_send_without_an_authenticated_agent_credential() {
        let (base_url, server) = test_server("first", "second").await;
        let error = RestClient::new(base_url)
            .get("/authorization/first")
            .await
            .unwrap_err();
        match error {
            ToolError::Mcp(error) => assert!(error
                .message
                .contains("without an authenticated agent credential")),
            ToolError::Grpc(_) => panic!("unexpected gRPC error"),
        }
        server.abort();
    }
}
