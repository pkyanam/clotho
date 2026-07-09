//! ComputeSDK bridge provider — optional TypeScript sidecar behind CCI.
//!
//! Does not replace the CCI. When `CLOTHO_COMPUTE_SDK_BRIDGE_URL` is set,
//! clotho-compute proxies one-shot jobs and provider metadata to a small HTTP
//! sidecar (`services/compute-sdk-bridge`) that may host ComputeSDK provider
//! packages (docs/adr/0013). Without the URL the provider is registered as
//! unconfigured so multi-provider listing stays honest.

use async_trait::async_trait;
use serde::Deserialize;

use crate::{
    ComputeError, ComputeProvider, JobResult, JobSpec, ProviderCapabilities, ProviderDescriptor,
    ProviderKind,
};

const PROVIDER_ID: &str = "computesdk";
const DEFAULT_TIMEOUT_SECS: u32 = 900;

pub struct ComputeSdkBridgeProvider {
    /// Base URL of the sidecar, e.g. `http://clotho-compute-sdk-bridge:8091`.
    bridge_url: Option<String>,
    http: reqwest::Client,
    /// Cached non-secret reason when unconfigured.
    reason: String,
}

impl ComputeSdkBridgeProvider {
    /// Build from env. Returns `None` when the bridge URL is unset so the
    /// registry can still register an explicit unconfigured instance via
    /// [`Self::unconfigured`].
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("CLOTHO_COMPUTE_SDK_BRIDGE_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())?;
        Some(Self::with_url(url))
    }

    pub fn with_url(url: impl Into<String>) -> Self {
        let bridge_url = url.into().trim_end_matches('/').to_string();
        Self {
            bridge_url: Some(bridge_url),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(1_200))
                .build()
                .expect("reqwest client"),
            reason: String::new(),
        }
    }

    pub fn unconfigured() -> Self {
        Self {
            bridge_url: None,
            http: reqwest::Client::new(),
            reason: "CLOTHO_COMPUTE_SDK_BRIDGE_URL not set; ComputeSDK bridge disabled".into(),
        }
    }

    fn provider(err: impl std::fmt::Display) -> ComputeError {
        ComputeError::Provider(err.to_string())
    }
}

#[derive(Debug, Deserialize)]
struct BridgeHealth {
    #[serde(default)]
    configured: bool,
    #[serde(default)]
    message: String,
    #[serde(default)]
    #[allow(dead_code)]
    providers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BridgeJobResponse {
    exit_code: i32,
    #[serde(default)]
    logs: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    sandbox_id: String,
}

#[async_trait]
impl ComputeProvider for ComputeSdkBridgeProvider {
    fn name(&self) -> &str {
        PROVIDER_ID
    }

    fn descriptor(&self) -> ProviderDescriptor {
        let (configured, reason, notes) = match &self.bridge_url {
            None => (
                false,
                self.reason.clone(),
                "optional TypeScript sidecar wrapping ComputeSDK providers".to_string(),
            ),
            Some(url) => (
                true,
                String::new(),
                format!("ComputeSDK bridge at {url}; upstream provider keys stay in the sidecar"),
            ),
        };

        ProviderDescriptor {
            id: PROVIDER_ID.into(),
            name: "ComputeSDK Bridge".into(),
            kind: ProviderKind::Bridge,
            configured,
            configured_reason: reason,
            capabilities: ProviderCapabilities {
                one_shot_jobs: true,
                persistent_workspaces: false,
                snapshots: true,
                templates: true,
                regions: vec![],
                ssh: false,
                desktop: false,
                public_url: false,
                file_api: true,
                terminal_streaming: false,
                cost_hints: "depends on upstream ComputeSDK provider".into(),
            },
            default_snapshot: String::new(),
            notes,
        }
    }

    async fn run_job(&self, spec: JobSpec) -> Result<JobResult, ComputeError> {
        let Some(base) = &self.bridge_url else {
            return Err(ComputeError::Disabled(self.reason.clone()));
        };
        if spec.commands.is_empty() {
            return Err(ComputeError::Invalid("no commands".into()));
        }

        // Best-effort health check so we fail with Disabled when the sidecar
        // has no upstream providers configured.
        let health_url = format!("{base}/health");
        if let Ok(resp) = self.http.get(&health_url).send().await {
            if let Ok(health) = resp.json::<BridgeHealth>().await {
                if !health.configured {
                    let msg = if health.message.is_empty() {
                        "ComputeSDK bridge has no configured upstream providers".into()
                    } else {
                        health.message
                    };
                    return Err(ComputeError::Disabled(msg));
                }
            }
        }

        let timeout = if spec.timeout_secs == 0 {
            DEFAULT_TIMEOUT_SECS
        } else {
            spec.timeout_secs
        };

        let files: Vec<serde_json::Value> = spec
            .files
            .iter()
            .map(|f| {
                serde_json::json!({
                    "path": f.path,
                    "content_base64": base64_encode(&f.content),
                })
            })
            .collect();

        let body = serde_json::json!({
            "label": spec.label,
            "snapshot": spec.snapshot,
            "commands": spec.commands,
            "env": spec.env,
            "timeout_secs": timeout,
            "files": files,
        });

        let url = format!("{base}/jobs");
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(Self::provider)?;
        let status = resp.status();
        let text = resp.text().await.map_err(Self::provider)?;
        if !status.is_success() {
            return Err(ComputeError::Provider(format!(
                "computesdk bridge job: {status}: {text}"
            )));
        }
        let parsed: BridgeJobResponse = serde_json::from_str(&text).map_err(|e| {
            ComputeError::Provider(format!("computesdk bridge bad JSON: {e}: {text}"))
        })?;

        Ok(JobResult {
            exit_code: parsed.exit_code,
            logs: parsed.logs,
            provider: if parsed.provider.is_empty() {
                PROVIDER_ID.into()
            } else {
                format!("{PROVIDER_ID}/{}", parsed.provider)
            },
            sandbox_id: parsed.sandbox_id,
        })
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    // Minimal base64 (no extra dependency): use a tiny hand-rolled encoder.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unconfigured_bridge_fails_cleanly() {
        let p = ComputeSdkBridgeProvider::unconfigured();
        assert!(!p.descriptor().configured);
        let err = p
            .run_job(JobSpec {
                commands: vec!["true".into()],
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert!(matches!(err, ComputeError::Disabled(_)));
    }
}
