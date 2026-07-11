//! Thin REST client for the api-gateway edge. No git/jj shell-out.

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

pub const EXIT_INTERNAL: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_AUTH: i32 = 3;
pub const EXIT_PERMISSION: i32 = 4;
pub const EXIT_CONFLICT: i32 = 5;
pub const EXIT_NOT_FOUND: i32 = 6;
pub const EXIT_UNAVAILABLE: i32 = 7;

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    request_id: String,
    #[serde(default)]
    retryable: bool,
}

#[derive(Debug)]
pub struct ApiFailure {
    pub status: u16,
    pub code: String,
    pub message: String,
    pub request_id: String,
    pub retryable: bool,
}

impl std::fmt::Display for ApiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if !self.code.is_empty() {
            write!(f, " [{}; HTTP {}]", self.code, self.status)?;
        } else {
            write!(f, " [HTTP {}]", self.status)?;
        }
        if !self.request_id.is_empty() {
            write!(f, " (request {})", self.request_id)?;
        }
        Ok(())
    }
}

impl std::error::Error for ApiFailure {}

impl ApiFailure {
    pub fn exit_code(&self) -> i32 {
        match self.code.as_str() {
            "unauthenticated" => EXIT_AUTH,
            "permission_denied" => EXIT_PERMISSION,
            "conflict" | "policy_conflict" | "idempotency_conflict" => EXIT_CONFLICT,
            "not_found" => EXIT_NOT_FOUND,
            "rate_limited"
            | "upstream_unavailable"
            | "service_unavailable"
            | "upstream_timeout" => EXIT_UNAVAILABLE,
            _ if self.retryable => EXIT_UNAVAILABLE,
            _ => EXIT_INTERNAL,
        }
    }
}

pub fn exit_code(error: &anyhow::Error) -> i32 {
    if let Some(api) = error.downcast_ref::<ApiFailure>() {
        return api.exit_code();
    }
    if let Some(http) = error.downcast_ref::<reqwest::Error>() {
        return if http.is_connect() || http.is_timeout() {
            EXIT_UNAVAILABLE
        } else {
            EXIT_INTERNAL
        };
    }
    let message = error.to_string();
    if message.starts_with("usage:")
        || message.starts_with("unknown ")
        || message.starts_with("unrecognized ")
        || message.starts_with("refusing ")
        || message.contains(" must be ")
        || message.contains(" requires ")
    {
        EXIT_USAGE
    } else {
        EXIT_INTERNAL
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub api_url: String,
    pub json: bool,
    pub token: Option<String>,
}

impl Config {
    pub fn from_env_and_args(api_url: String, json: bool, token: Option<String>) -> Self {
        Self {
            api_url: api_url.trim_end_matches('/').to_string(),
            json,
            token,
        }
    }
}

pub async fn request_json<T: DeserializeOwned>(
    config: &Config,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<T> {
    let value = request_value(config, method, path, body).await?;
    serde_json::from_value(value).with_context(|| format!("decode response from {path}"))
}

pub async fn request_value(
    config: &Config,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value> {
    request_value_with_headers(config, method, path, body, &[]).await
}

pub async fn request_value_with_headers(
    config: &Config,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> Result<Value> {
    let client = reqwest::Client::new();
    let mut request = client.request(method.clone(), format!("{}{}", config.api_url, path));
    if let Some(token) = &config.token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request.send().await?;
    let status = response.status();
    let header_request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let text = response.text().await?;
    if status.as_u16() == 204 || text.is_empty() {
        if status.is_success() {
            return Ok(Value::Null);
        }
        bail!("{method} {path} failed: {status}");
    }
    if !status.is_success() {
        let legacy = serde_json::from_str::<Value>(&text).ok();
        let envelope = serde_json::from_str::<ErrorEnvelope>(&text).unwrap_or(ErrorEnvelope {
            code: "http_error".into(),
            message: legacy
                .as_ref()
                .and_then(|value| value.get("error"))
                .and_then(|value| value.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{method} {path} failed: {status}")),
            request_id: legacy
                .as_ref()
                .and_then(|value| value.get("request_id"))
                .and_then(|value| value.as_str())
                .map(str::to_owned)
                .unwrap_or(header_request_id),
            retryable: false,
        });
        return Err(ApiFailure {
            status: status.as_u16(),
            code: envelope.code,
            message: envelope.message,
            request_id: envelope.request_id,
            retryable: envelope.retryable,
        }
        .into());
    }
    serde_json::from_str(&text).with_context(|| format!("decode JSON from {path}"))
}

/// Print JSON when `--json` is set; otherwise run the human formatter.
pub fn emit(config: &Config, value: &Value, human: impl FnOnce()) -> Result<()> {
    if config.json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        human();
    }
    Ok(())
}

pub fn short(commit_id: &str) -> &str {
    commit_id.get(..12).unwrap_or(commit_id)
}

pub fn first_line(message: &str) -> &str {
    message.lines().next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_api_exit_classes() {
        let failure = |code: &str, retryable: bool| ApiFailure {
            status: 400,
            code: code.into(),
            message: "failed".into(),
            request_id: "req-1".into(),
            retryable,
        };
        assert_eq!(failure("unauthenticated", false).exit_code(), EXIT_AUTH);
        assert_eq!(
            failure("permission_denied", false).exit_code(),
            EXIT_PERMISSION
        );
        assert_eq!(failure("conflict", false).exit_code(), EXIT_CONFLICT);
        assert_eq!(
            failure("idempotency_conflict", false).exit_code(),
            EXIT_CONFLICT
        );
        assert_eq!(failure("not_found", false).exit_code(), EXIT_NOT_FOUND);
        assert_eq!(
            failure("upstream_unavailable", true).exit_code(),
            EXIT_UNAVAILABLE
        );
    }
}
