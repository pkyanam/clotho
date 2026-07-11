//! Box (Ascii) provider — persistent agent workspace / one-shot CI sandbox.
//!
//! Real CCI integration against Box Public API v1
//! (https://docs.ascii.dev/box/api/v1, base `https://ascii.dev/api/box/v1`):
//!
//! - Auth: `Authorization: Bearer $BOX_API_KEY` (or per-job
//!   `provider_credentials.api_key` from Clotho secrets, docs/adr/0014)
//! - One-shot jobs: create → poll ready/idle → write files → run commands → delete
//! - Persistent hooks: create/stop/resume/delete helpers for a later session API
//!
//! `configured` is true only when a process env key is present. Clotho secrets
//! are overlaid by the api-gateway for settings; jobs with injected credentials
//! still run when env is empty (same pattern as Daytona).

use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;

use crate::{
    ComputeError, ComputeProvider, JobResult, JobSpec, ProviderCapabilities, ProviderDescriptor,
    ProviderKind,
};

const PROVIDER_ID: &str = "box";
/// Public Box API v1 base (docs.ascii.dev/box/api/v1).
const DEFAULT_API_URL: &str = "https://ascii.dev/api/box/v1";
/// How long to wait for a freshly created box to become ready/idle.
const READY_TIMEOUT: Duration = Duration::from_secs(300);
const READY_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Default per-command timeout when a job doesn't set one.
/// Box API caps `timeoutSeconds` at 60 (docs).
const DEFAULT_CMD_TIMEOUT_SECS: u32 = 60;
const BOX_CMD_TIMEOUT_MAX: u32 = 60;

#[derive(Clone)]
pub struct BoxConfig {
    pub api_key: String,
    pub api_url: String,
    /// TTL for one-shot boxes (seconds). `None` means no auto-archive (persist).
    pub default_ttl_secs: Option<u32>,
}

/// Real Box HTTP provider (Stage 14).
pub struct BoxProvider {
    config: BoxConfig,
    http: reqwest::Client,
}

impl BoxProvider {
    /// Prefer [`Self::from_env_or_unconfigured`] so Clotho secrets can supply
    /// per-job keys when process env is empty.
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("BOX_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty())?;
        Some(Self::from_key(api_key))
    }

    /// Always construct a Box provider. Empty env key → unconfigured at list
    /// time, but jobs with `provider_credentials.api_key` still run.
    pub fn from_env_or_unconfigured() -> Self {
        let api_key = std::env::var("BOX_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty())
            .unwrap_or_default();
        Self::from_key(api_key)
    }

    fn from_key(api_key: String) -> Self {
        let api_url = std::env::var("BOX_API_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_API_URL.into());
        let default_ttl_secs = std::env::var("BOX_DEFAULT_TTL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(Some(3600));
        Self::new(BoxConfig {
            api_key,
            api_url,
            default_ttl_secs,
        })
    }

    pub fn new(config: BoxConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(1_200))
            .build()
            .expect("reqwest client");
        Self {
            config: BoxConfig {
                api_url: config.api_url.trim_end_matches('/').to_string(),
                ..config
            },
            http,
        }
    }

    /// Prefer per-job credential from Clotho secrets over process env.
    fn resolve_api_key(&self, spec: &JobSpec) -> Result<String, ComputeError> {
        if let Some(key) = spec
            .provider_credentials
            .get("api_key")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            return Ok(key.to_string());
        }
        if !self.config.api_key.is_empty() {
            return Ok(self.config.api_key.clone());
        }
        Err(ComputeError::Disabled(
            "Box is not connected — add an API key in Clotho settings (compute), or set BOX_API_KEY for local dev"
                .into(),
        ))
    }

    fn provider(err: impl std::fmt::Display) -> ComputeError {
        ComputeError::Provider(err.to_string())
    }

    fn auth(&self, builder: reqwest::RequestBuilder, api_key: &str) -> reqwest::RequestBuilder {
        builder.bearer_auth(api_key)
    }

    /// Create a one-shot box (auto-archives after TTL when set).
    async fn create_box(
        &self,
        api_key: &str,
        env: &std::collections::HashMap<String, String>,
    ) -> Result<String, ComputeError> {
        let mut body = json!({});
        if let Some(ttl) = self.config.default_ttl_secs {
            body["ttlSeconds"] = json!(ttl);
        } else {
            body["ttlSeconds"] = serde_json::Value::Null;
        }
        if !env.is_empty() {
            // Only inject simple env keys that match Box constraints.
            let mut clean = serde_json::Map::new();
            for (k, v) in env {
                if is_safe_env_key(k) {
                    clean.insert(k.clone(), json!(v));
                }
            }
            if !clean.is_empty() {
                body["env"] = serde_json::Value::Object(clean);
            }
        }
        let url = format!("{}/boxes", self.config.api_url);
        let resp = self
            .auth(self.http.post(&url).json(&body), api_key)
            .send()
            .await
            .map_err(Self::provider)?;
        let value = Self::json_or_err(resp, "create box").await?;
        let id = value
            .pointer("/box/id")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("id").and_then(|v| v.as_str()))
            .ok_or_else(|| ComputeError::Provider("create box: no box.id in response".into()))?;
        Ok(id.to_string())
    }

    /// Hook for persistent workspaces: create without TTL (no auto-archive).
    #[allow(dead_code)]
    pub async fn create_persistent(
        &self,
        api_key: &str,
        env: &std::collections::HashMap<String, String>,
    ) -> Result<String, ComputeError> {
        let mut body = json!({ "ttlSeconds": null });
        if !env.is_empty() {
            let mut clean = serde_json::Map::new();
            for (k, v) in env {
                if is_safe_env_key(k) {
                    clean.insert(k.clone(), json!(v));
                }
            }
            if !clean.is_empty() {
                body["env"] = serde_json::Value::Object(clean);
            }
        }
        let url = format!("{}/boxes", self.config.api_url);
        let resp = self
            .auth(self.http.post(&url).json(&body), api_key)
            .send()
            .await
            .map_err(Self::provider)?;
        let value = Self::json_or_err(resp, "create persistent box").await?;
        let id = value
            .pointer("/box/id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ComputeError::Provider("create persistent box: no box.id in response".into())
            })?;
        Ok(id.to_string())
    }

    async fn wait_ready(&self, api_key: &str, id: &str) -> Result<(), ComputeError> {
        let deadline = std::time::Instant::now() + READY_TIMEOUT;
        let url = format!("{}/boxes/{id}", self.config.api_url);
        loop {
            let resp = self
                .auth(self.http.get(&url), api_key)
                .send()
                .await
                .map_err(Self::provider)?;
            let value = Self::json_or_err(resp, "get box").await?;
            let state = value
                .pointer("/box/state")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match state {
                "ready" | "idle" => return Ok(()),
                "error" => {
                    return Err(ComputeError::Provider(format!(
                        "box {id} entered state error"
                    )));
                }
                "archived" | "archiving" => {
                    return Err(ComputeError::Provider(format!(
                        "box {id} became {state} before ready"
                    )));
                }
                _ => {}
            }
            if std::time::Instant::now() >= deadline {
                return Err(ComputeError::Provider(format!(
                    "box {id} not ready within {}s (last state {state:?})",
                    READY_TIMEOUT.as_secs()
                )));
            }
            tokio::time::sleep(READY_POLL_INTERVAL).await;
        }
    }

    /// Box file paths are relative to the box work directory.
    fn relative_path(path: &str) -> String {
        let p = path.trim_start_matches('/');
        // Strip common absolute workdir prefixes used by CI packaging.
        // Prefer keeping a useful relative suffix (e.g. clotho-ci/repo.tar).
        for prefix in [
            "tmp/clotho-ci/",
            "tmp/",
            "workspace/",
            "home/user/",
            "home/",
        ] {
            if let Some(rest) = p.strip_prefix(prefix) {
                return rest.to_string();
            }
        }
        p.to_string()
    }

    async fn write_file(
        &self,
        api_key: &str,
        id: &str,
        path: &str,
        content: &[u8],
    ) -> Result<(), ComputeError> {
        let rel = Self::relative_path(path);
        let url = format!("{}/boxes/{id}/files", self.config.api_url);
        // Binary-safe: always send base64. Box API paths are relative to the
        // work directory; stage under a relative name first.
        let content_b64 = base64_encode(content);
        let body = json!({
            "path": rel,
            "content": content_b64,
            "encoding": "base64",
        });
        let resp = self
            .auth(self.http.put(&url).json(&body), api_key)
            .send()
            .await
            .map_err(Self::provider)?;
        Self::ok_or_err(resp, "write file").await?;

        // CI packages absolute paths (e.g. /tmp/clotho-ci/repo.tar). Copy the
        // staged relative file into place so existing CI scripts keep working.
        if path.starts_with('/') && path != rel {
            let parent = path.rsplit_once('/').map(|(p, _)| p).unwrap_or("/");
            let cmd = format!("mkdir -p '{parent}' && cp -f -- '{rel}' '{path}'");
            let (code, out) = self
                .execute(api_key, id, &cmd, &Default::default(), 30)
                .await?;
            if code != 0 {
                return Err(ComputeError::Provider(format!(
                    "box stage file {path}: exit {code}: {out}"
                )));
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        api_key: &str,
        id: &str,
        command: &str,
        env: &std::collections::HashMap<String, String>,
        timeout_secs: u32,
    ) -> Result<(i32, String), ComputeError> {
        let url = format!("{}/boxes/{id}/commands", self.config.api_url);
        let full_command = if env.is_empty() {
            command.to_string()
        } else {
            let mut prefix = String::new();
            for (key, value) in env {
                let escaped = value.replace('\'', r"'\''");
                prefix.push_str(&format!("export {key}='{escaped}'; "));
            }
            format!("{prefix}{command}")
        };
        let timeout = timeout_secs.clamp(1, BOX_CMD_TIMEOUT_MAX);
        let body = json!({
            "command": full_command,
            "timeoutSeconds": timeout,
        });
        let resp = self
            .auth(self.http.post(&url).json(&body), api_key)
            .send()
            .await
            .map_err(Self::provider)?;
        let value = Self::json_or_err(resp, "execute command").await?;
        let exit_code = value.get("exitCode").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
        let stdout = value.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
        let stderr = value.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
        let timed_out = value
            .get("timedOut")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut output = String::new();
        if !stdout.is_empty() {
            output.push_str(stdout);
        }
        if !stderr.is_empty() {
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(stderr);
        }
        if timed_out {
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(&format!(
                "box command timed out after {timeout}s (Box API max {BOX_CMD_TIMEOUT_MAX}s)\n"
            ));
        }
        Ok((exit_code, output))
    }

    /// Hook: stop/archive a persistent box (not used by one-shot run_job).
    #[allow(dead_code)]
    pub async fn stop_box(&self, api_key: &str, id: &str) -> Result<(), ComputeError> {
        let url = format!("{}/boxes/{id}/stop", self.config.api_url);
        let resp = self
            .auth(self.http.post(&url), api_key)
            .send()
            .await
            .map_err(Self::provider)?;
        Self::ok_or_err(resp, "stop box").await
    }

    /// Hook: resume an archived box.
    #[allow(dead_code)]
    pub async fn resume_box(&self, api_key: &str, id: &str) -> Result<(), ComputeError> {
        let url = format!("{}/boxes/{id}/resume", self.config.api_url);
        let resp = self
            .auth(self.http.post(&url), api_key)
            .send()
            .await
            .map_err(Self::provider)?;
        Self::ok_or_err(resp, "resume box").await
    }

    async fn delete_box(&self, api_key: &str, id: &str) {
        let url = format!("{}/boxes/{id}", self.config.api_url);
        match self.auth(self.http.delete(&url), api_key).send().await {
            Ok(resp) if resp.status().is_success() => {}
            Ok(resp) => {
                tracing::warn!(box_id = id, status = %resp.status(), "box delete failed")
            }
            Err(e) => tracing::warn!(box_id = id, error = %e, "box delete failed"),
        }
    }

    async fn json_or_err(
        resp: reqwest::Response,
        what: &str,
    ) -> Result<serde_json::Value, ComputeError> {
        let status = resp.status();
        let text = resp.text().await.map_err(Self::provider)?;
        if !status.is_success() {
            return Err(ComputeError::Provider(format!(
                "box {what}: {status}: {text}"
            )));
        }
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| ComputeError::Provider(format!("box {what}: bad JSON: {e}: {text}")))?;
        if value.get("ok").and_then(|v| v.as_bool()) == Some(false) {
            let msg = value
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or(&text);
            return Err(ComputeError::Provider(format!("box {what}: {msg}")));
        }
        Ok(value)
    }

    async fn ok_or_err(resp: reqwest::Response, what: &str) -> Result<(), ComputeError> {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(ComputeError::Provider(format!(
                "box {what}: {status}: {text}"
            )));
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if value.get("ok").and_then(|v| v.as_bool()) == Some(false) {
                let msg = value
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&text);
                return Err(ComputeError::Provider(format!("box {what}: {msg}")));
            }
        }
        Ok(())
    }

    async fn run_on_box(
        &self,
        api_key: &str,
        id: &str,
        spec: &JobSpec,
        timeout: u32,
    ) -> Result<(i32, String), ComputeError> {
        self.wait_ready(api_key, id).await?;
        for file in &spec.files {
            self.write_file(api_key, id, &file.path, &file.content)
                .await?;
        }
        let mut logs = String::new();
        let mut exit_code = 0;
        for command in &spec.commands {
            let (code, output) = self
                .execute(api_key, id, command, &spec.env, timeout)
                .await?;
            logs.push_str(&output);
            if !output.ends_with('\n') {
                logs.push('\n');
            }
            exit_code = code;
            if code != 0 {
                break;
            }
        }
        Ok((exit_code, logs))
    }
}

#[async_trait]
impl ComputeProvider for BoxProvider {
    fn name(&self) -> &str {
        PROVIDER_ID
    }

    fn descriptor(&self) -> ProviderDescriptor {
        let configured = !self.config.api_key.is_empty();
        ProviderDescriptor {
            id: PROVIDER_ID.into(),
            name: "Box".into(),
            kind: ProviderKind::Direct,
            configured,
            configured_reason: if configured {
                String::new()
            } else {
                "not connected — add an API key in Clotho settings or set BOX_API_KEY for local dev"
                    .into()
            },
            capabilities: ProviderCapabilities {
                one_shot_jobs: true,
                persistent_workspaces: true,
                snapshots: true,
                templates: false,
                regions: vec![],
                ssh: true,
                desktop: true,
                public_url: true,
                file_api: true,
                terminal_streaming: true,
                gpu: false,
                gpu_types: vec![],
                cost_hints: "persistent Ubuntu VM; TTL / pay-per-use (see Box dashboard)".into(),
            },
            default_snapshot: String::new(),
            notes: format!(
                "ascii Box API v1 at {}; one-shot create→files→commands→delete; \
                 per-command timeout max {}s; credentials from Clotho secrets or process env",
                self.config.api_url, BOX_CMD_TIMEOUT_MAX
            ),
        }
    }

    async fn run_job(&self, spec: JobSpec) -> Result<JobResult, ComputeError> {
        if spec.commands.is_empty() {
            return Err(ComputeError::Invalid("no commands".into()));
        }
        let api_key = self.resolve_api_key(&spec)?;
        let timeout = if spec.timeout_secs == 0 {
            DEFAULT_CMD_TIMEOUT_SECS
        } else {
            spec.timeout_secs.min(BOX_CMD_TIMEOUT_MAX)
        };

        tracing::info!(job = %spec.label, "creating box sandbox");
        let id = self.create_box(&api_key, &spec.env).await?;
        let result = self.run_on_box(&api_key, &id, &spec, timeout).await;
        self.delete_box(&api_key, &id).await;
        let (exit_code, logs) = result?;

        Ok(JobResult {
            exit_code,
            logs,
            provider: PROVIDER_ID.into(),
            sandbox_id: id,
        })
    }
}

fn is_safe_env_key(k: &str) -> bool {
    let mut chars = k.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_') && k.len() <= 128
}

fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(T[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(T[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Backward-compatible alias for older call sites / docs.
pub type BoxStubProvider = BoxProvider;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unconfigured_rejects_jobs_honestly() {
        let p = BoxProvider::new(BoxConfig {
            api_key: String::new(),
            api_url: DEFAULT_API_URL.into(),
            default_ttl_secs: Some(3600),
        });
        let d = p.descriptor();
        assert_eq!(d.id, "box");
        assert!(!d.configured);
        assert!(d.capabilities.persistent_workspaces);
        assert!(d.capabilities.ssh);
        assert!(d.capabilities.one_shot_jobs);
        assert_eq!(d.kind, ProviderKind::Direct);
        assert!(d.notes.contains("ascii.dev/api/box/v1"));
        let err = p
            .run_job(JobSpec {
                commands: vec!["true".into()],
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert!(matches!(err, ComputeError::Disabled(_)));
    }

    #[test]
    fn relative_path_strips_workdir_prefixes() {
        assert_eq!(
            BoxProvider::relative_path("/workspace/repo.tar"),
            "repo.tar"
        );
        assert_eq!(BoxProvider::relative_path("repo.tar"), "repo.tar");
        assert_eq!(BoxProvider::relative_path("/tmp/ci.sh"), "ci.sh");
    }

    #[test]
    fn base64_roundtrip_smoke() {
        let s = base64_encode(b"hello");
        assert_eq!(s, "aGVsbG8=");
    }
}
