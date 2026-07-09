//! Thin HTTP client for the api-gateway REST edge.
//!
//! Stage 15 MCP collab/Actions/platform tools call this surface so agents
//! cannot drift from the public product contract (docs/openapi.yaml).

use reqwest::StatusCode;
use serde_json::Value;

use crate::mcp::ToolError;

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
        self.request(reqwest::Method::GET, path, None).await
    }

    pub async fn post(&self, path: &str, body: Value) -> Result<Value, ToolError> {
        self.request(reqwest::Method::POST, path, Some(body)).await
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, ToolError> {
        let url = format!("{}{path}", self.base_url);
        let mut req = self.http.request(method.clone(), &url);
        if let Some(body) = body {
            req = req.json(&body);
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
            return Err(map_status(status, &format!("{method} {path} failed")));
        }

        let value: Value = serde_json::from_str(&text).unwrap_or_else(|_| {
            Value::String(text.clone())
        });

        if !status.is_success() {
            let msg = value
                .get("error")
                .and_then(|e| e.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| text.clone());
            return Err(map_status(status, &msg));
        }
        Ok(value)
    }
}

fn map_status(status: StatusCode, message: &str) -> ToolError {
    use rmcp::ErrorData as McpError;
    let err = if status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND {
        McpError::invalid_params(message.to_string(), None)
    } else {
        McpError::internal_error(format!("api-gateway {status}: {message}"), None)
    };
    ToolError::Mcp(err)
}
