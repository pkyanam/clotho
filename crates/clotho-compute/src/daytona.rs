//! Daytona provider for the CCI (docs/adr/0008).
//!
//! Talks Daytona's public REST API directly — no TS SDK in the loop. Two
//! surfaces, both Bearer-authenticated with `DAYTONA_API_KEY`:
//!
//! - **control plane** (`https://app.daytona.io/api`): create / poll / delete
//!   sandboxes.
//! - **toolbox proxy** (`https://proxy.app.daytona.io/toolbox/{id}`):
//!   `process/execute` and `files/upload` inside a running sandbox.
//!
//! A job creates a fresh sandbox from a snapshot, uploads the staged files,
//! runs the commands in order (stopping at the first non-zero exit), then
//! deletes the sandbox — always, even on failure.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use serde_json::json;

use crate::{
    ComputeError, ComputeProvider, JobResult, JobSpec, ProviderCapabilities, ProviderDescriptor,
    ProviderKind,
};

const DEFAULT_API_URL: &str = "https://app.daytona.io/api";
const DEFAULT_PROXY_URL: &str = "https://proxy.app.daytona.io";
const DEFAULT_SNAPSHOT: &str = "daytona-small";
/// How long to wait for a freshly created sandbox to reach `started`.
const START_TIMEOUT: Duration = Duration::from_secs(180);
const START_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Default per-command timeout when a job doesn't set one (CI can be slow).
const DEFAULT_CMD_TIMEOUT_SECS: u32 = 900;

#[derive(Clone)]
pub struct DaytonaConfig {
    pub api_key: String,
    /// Control-plane base, e.g. `https://app.daytona.io/api`.
    pub api_url: String,
    /// Toolbox proxy base, e.g. `https://proxy.app.daytona.io`.
    pub proxy_url: String,
    /// Snapshot launched when a job doesn't specify one.
    pub default_snapshot: String,
    /// Region routing (`us`/`eu`); sent on create only when non-empty.
    pub target: String,
    /// Multi-org key routing; sent as `X-Daytona-Organization-ID` when set.
    pub organization_id: String,
}

pub struct DaytonaProvider {
    config: DaytonaConfig,
    http: reqwest::Client,
}

impl DaytonaProvider {
    /// Build a provider from the environment, or `None` when `DAYTONA_API_KEY`
    /// is unset (so the service can fall back to the disabled provider without
    /// a credential — docs/adr/0008). Prefer [`Self::from_env_or_unconfigured`]
    /// so Clotho secrets can supply per-job keys (docs/adr/0014).
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("DAYTONA_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())?;
        Some(Self::from_key(api_key))
    }

    /// Always construct a Daytona provider. When env key is empty the provider
    /// is listed as unconfigured but can still run jobs that carry
    /// `provider_credentials.api_key` from the api-gateway secrets store.
    pub fn from_env_or_unconfigured() -> Self {
        let api_key = std::env::var("DAYTONA_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .unwrap_or_default();
        Self::from_key(api_key)
    }

    fn from_key(api_key: String) -> Self {
        let env_or = |name: &str, default: &str| {
            std::env::var(name)
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| default.to_string())
        };
        Self::new(DaytonaConfig {
            api_key,
            api_url: env_or("DAYTONA_API_URL", DEFAULT_API_URL),
            proxy_url: env_or("DAYTONA_PROXY_URL", DEFAULT_PROXY_URL),
            default_snapshot: env_or("CLOTHO_COMPUTE_SNAPSHOT", DEFAULT_SNAPSHOT),
            target: std::env::var("DAYTONA_TARGET").unwrap_or_default(),
            organization_id: std::env::var("DAYTONA_ORGANIZATION_ID").unwrap_or_default(),
        })
    }

    pub fn new(config: DaytonaConfig) -> Self {
        let http = reqwest::Client::builder()
            // CI commands can run for many minutes; keep the client from
            // cutting them short (the exec call carries its own timeout too).
            .timeout(Duration::from_secs(1_200))
            .build()
            .expect("reqwest client");
        Self {
            config: DaytonaConfig {
                api_url: config.api_url.trim_end_matches('/').to_string(),
                proxy_url: config.proxy_url.trim_end_matches('/').to_string(),
                ..config
            },
            http,
        }
    }

    /// Prefer per-job credential from Clotho secrets over process env.
    fn resolve_api_key(&self, spec: &crate::JobSpec) -> Result<String, ComputeError> {
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
            "Daytona is not connected — add an API key in Clotho settings (compute), or set DAYTONA_API_KEY for local dev"
                .into(),
        ))
    }

    fn auth_with_key(
        &self,
        builder: reqwest::RequestBuilder,
        api_key: &str,
    ) -> reqwest::RequestBuilder {
        let mut b = builder.bearer_auth(api_key);
        if !self.config.organization_id.is_empty() {
            b = b.header("X-Daytona-Organization-ID", &self.config.organization_id);
        }
        b
    }

    fn auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        self.auth_with_key(builder, &self.config.api_key)
    }

    fn provider(err: impl std::fmt::Display) -> ComputeError {
        ComputeError::Provider(err.to_string())
    }

    async fn create_sandbox(&self, snapshot: &str) -> Result<String, ComputeError> {
        let mut body = json!({ "snapshot": snapshot });
        if !self.config.target.is_empty() {
            body["target"] = json!(self.config.target);
        }
        let url = format!("{}/sandbox", self.config.api_url);
        let resp = self
            .auth(self.http.post(&url).json(&body))
            .send()
            .await
            .map_err(Self::provider)?;
        let value = Self::json_or_err(resp, "create sandbox").await?;
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ComputeError::Provider("create sandbox: no id in response".into()))?;
        Ok(id.to_string())
    }

    async fn wait_started(&self, id: &str) -> Result<(), ComputeError> {
        let deadline = std::time::Instant::now() + START_TIMEOUT;
        let url = format!("{}/sandbox/{id}", self.config.api_url);
        loop {
            let resp = self
                .auth(self.http.get(&url))
                .send()
                .await
                .map_err(Self::provider)?;
            let value = Self::json_or_err(resp, "get sandbox").await?;
            let state = value.get("state").and_then(|v| v.as_str()).unwrap_or("");
            match state {
                "started" => return Ok(()),
                "error" | "build_failed" | "destroyed" | "destroying" => {
                    let msg = value
                        .get("errorReason")
                        .and_then(|v| v.as_str())
                        .unwrap_or(state);
                    return Err(ComputeError::Provider(format!(
                        "sandbox {id} entered state {state}: {msg}"
                    )));
                }
                _ => {}
            }
            if std::time::Instant::now() >= deadline {
                return Err(ComputeError::Provider(format!(
                    "sandbox {id} not started within {}s (last state {state:?})",
                    START_TIMEOUT.as_secs()
                )));
            }
            tokio::time::sleep(START_POLL_INTERVAL).await;
        }
    }

    async fn ensure_parent_dir(&self, id: &str, path: &str) {
        let Some(parent) = path
            .rsplit_once('/')
            .map(|(p, _)| p)
            .filter(|p| !p.is_empty())
        else {
            return;
        };
        let url = format!(
            "{}/toolbox/{id}/files/folder?path={}&mode=0755",
            self.config.proxy_url,
            urlencode(parent)
        );
        // Best-effort: the folder may already exist; upload will surface real
        // failures.
        let _ = self.auth(self.http.post(&url)).send().await;
    }

    async fn upload_file(
        &self,
        id: &str,
        path: &str,
        content: Vec<u8>,
    ) -> Result<(), ComputeError> {
        self.ensure_parent_dir(id, path).await;
        let url = format!(
            "{}/toolbox/{id}/files/upload?path={}",
            self.config.proxy_url,
            urlencode(path)
        );
        let form = Form::new().part(
            "file",
            Part::bytes(content).file_name(path.rsplit('/').next().unwrap_or("upload").to_string()),
        );
        let resp = self
            .auth(self.http.post(&url).multipart(form))
            .send()
            .await
            .map_err(Self::provider)?;
        Self::ok_or_err(resp, "upload file").await
    }

    /// Run one shell command; returns (exit_code, combined_output).
    ///
    /// Environment is folded into the command as leading `export`s rather than
    /// sent as a body field: Daytona's `process/execute` ignores an `env`/`envs`
    /// field (verified against the live API), and prepending exports is
    /// portable across any provider.
    async fn execute(
        &self,
        id: &str,
        command: &str,
        env: &std::collections::HashMap<String, String>,
        timeout_secs: u32,
    ) -> Result<(i32, String), ComputeError> {
        let url = format!("{}/toolbox/{id}/process/execute", self.config.proxy_url);
        let full_command = if env.is_empty() {
            command.to_string()
        } else {
            let mut prefix = String::new();
            for (key, value) in env {
                // POSIX single-quote escaping: close, escaped quote, reopen.
                let escaped = value.replace('\'', r"'\''");
                prefix.push_str(&format!("export {key}='{escaped}'; "));
            }
            format!("{prefix}{command}")
        };
        let body = json!({ "command": full_command, "timeout": timeout_secs });
        let resp = self
            .auth(self.http.post(&url).json(&body))
            .send()
            .await
            .map_err(Self::provider)?;
        let value = Self::json_or_err(resp, "execute command").await?;
        let exit_code = value.get("exitCode").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
        // Different Daytona versions name the output field `result` or `output`.
        let output = value
            .get("result")
            .or_else(|| value.get("output"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok((exit_code, output))
    }

    async fn delete_sandbox(&self, id: &str) {
        let url = format!("{}/sandbox/{id}?force=true", self.config.api_url);
        match self.auth(self.http.delete(&url)).send().await {
            Ok(resp) if resp.status().is_success() => {}
            Ok(resp) => {
                tracing::warn!(sandbox = id, status = %resp.status(), "sandbox delete failed")
            }
            Err(e) => tracing::warn!(sandbox = id, error = %e, "sandbox delete failed"),
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
                "daytona {what}: {status}: {text}"
            )));
        }
        serde_json::from_str(&text)
            .map_err(|e| ComputeError::Provider(format!("daytona {what}: bad JSON: {e}: {text}")))
    }

    async fn ok_or_err(resp: reqwest::Response, what: &str) -> Result<(), ComputeError> {
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = resp.text().await.unwrap_or_default();
            Err(ComputeError::Provider(format!(
                "daytona {what}: {status}: {text}"
            )))
        }
    }
}

#[async_trait]
impl ComputeProvider for DaytonaProvider {
    fn name(&self) -> &str {
        "daytona"
    }

    fn descriptor(&self) -> ProviderDescriptor {
        let mut regions = Vec::new();
        if !self.config.target.is_empty() {
            regions.push(self.config.target.clone());
        }
        let configured = !self.config.api_key.is_empty();
        ProviderDescriptor {
            id: "daytona".into(),
            name: "Daytona".into(),
            kind: ProviderKind::Direct,
            configured,
            configured_reason: if configured {
                String::new()
            } else {
                "not connected — add an API key in Clotho settings or set DAYTONA_API_KEY for local dev"
                    .into()
            },
            capabilities: ProviderCapabilities {
                one_shot_jobs: true,
                persistent_workspaces: true,
                snapshots: true,
                templates: false,
                regions,
                ssh: false,
                desktop: false,
                public_url: false,
                file_api: true,
                terminal_streaming: false,
                gpu: true,
                gpu_types: vec![
                    "H100".into(),
                    "H200".into(),
                    "RTX-PRO-6000".into(),
                    "RTX-4090".into(),
                    "RTX-5090".into(),
                ],
                cost_hints: "Daytona cloud sandbox (API-key billed)".into(),
            },
            default_snapshot: self.config.default_snapshot.clone(),
            notes: "direct Rust provider; credentials from Clotho secrets or process env".into(),
        }
    }

    async fn run_job(&self, spec: JobSpec) -> Result<JobResult, ComputeError> {
        if spec.commands.is_empty() {
            return Err(ComputeError::Invalid("no commands".into()));
        }
        // Per-job key from api-gateway secrets overrides process env (docs/adr/0014).
        let api_key = self.resolve_api_key(&spec)?;
        let runner = if api_key == self.config.api_key {
            None
        } else {
            Some(Self::new(DaytonaConfig {
                api_key,
                ..self.config.clone()
            }))
        };
        let this = runner.as_ref().unwrap_or(self);

        let snapshot = if spec.snapshot.is_empty() {
            this.config.default_snapshot.clone()
        } else {
            spec.snapshot.clone()
        };
        let timeout = if spec.timeout_secs == 0 {
            DEFAULT_CMD_TIMEOUT_SECS
        } else {
            spec.timeout_secs
        };

        tracing::info!(job = %spec.label, %snapshot, "creating daytona sandbox");
        let id = this.create_sandbox(&snapshot).await?;
        // From here on always tear the sandbox down.
        let result = this.run_on_sandbox(&id, &spec, timeout).await;
        this.delete_sandbox(&id).await;
        let (exit_code, logs) = result?;

        Ok(JobResult {
            exit_code,
            logs,
            provider: self.name().to_string(),
            sandbox_id: id,
        })
    }
}

impl DaytonaProvider {
    async fn run_on_sandbox(
        &self,
        id: &str,
        spec: &JobSpec,
        timeout: u32,
    ) -> Result<(i32, String), ComputeError> {
        self.wait_started(id).await?;
        for file in &spec.files {
            self.upload_file(id, &file.path, file.content.clone())
                .await?;
        }
        let mut logs = String::new();
        let mut exit_code = 0;
        for command in &spec.commands {
            let (code, output) = self.execute(id, command, &spec.env, timeout).await?;
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

/// Minimal percent-encoding for URL query values (paths may contain `/`, which
/// we keep, plus spaces and a few reserved chars we don't).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
