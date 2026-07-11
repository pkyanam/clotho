//! Thin REST client for the api-gateway edge. No git/jj shell-out.

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;

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
    let client = reqwest::Client::new();
    let mut request = client.request(method.clone(), format!("{}{}", config.api_url, path));
    if let Some(token) = &config.token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await?;
    let status = response.status();
    let text = response.text().await?;
    if status.as_u16() == 204 || text.is_empty() {
        if status.is_success() {
            return Ok(Value::Null);
        }
        bail!("{method} {path} failed: {status}");
    }
    if !status.is_success() {
        // Prefer gateway error envelope when present.
        if let Ok(err) = serde_json::from_str::<Value>(&text) {
            if let Some(msg) = err.get("error").and_then(|e| e.as_str()) {
                bail!("{method} {path} failed: {status}: {msg}");
            }
        }
        bail!("{method} {path} failed: {status}: {text}");
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
