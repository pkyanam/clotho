//! Thin HTTP client for the api-gateway REST edge.
//!
//! Stage 15 MCP collab/Actions/platform tools call this surface so agents
//! cannot drift from the public product contract (docs/openapi.yaml).

use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::Value;

use crate::mcp::ToolError;

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
        let mut req = self.http.request(method.clone(), &url);
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
