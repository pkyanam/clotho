//! ComputeSDK bridge provider — optional TypeScript sidecar behind CCI.
//!
//! Does not replace the CCI. When `CLOTHO_COMPUTE_SDK_BRIDGE_URL` is set,
//! clotho-compute proxies one-shot jobs and provider metadata to a small HTTP
//! sidecar (`services/compute-sdk-bridge`) that may host ComputeSDK provider
//! packages (docs/adr/0013). Without the URL the provider is registered as
//! unconfigured so multi-provider listing stays honest.
//!
//! **Configured means jobs can run:** URL alone is not enough. We probe
//! `/health` (live descriptor / job path) and also accept per-job upstream
//! credentials from Clotho secrets (`e2b_api_key`, `modal_token_id`, …)
//! forwarded in the job body so the bridge need not hold only host env keys.

use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Mutex;

use crate::{
    ComputeError, ComputeProvider, JobResult, JobSpec, ProviderCapabilities, ProviderDescriptor,
    ProviderKind,
};

const PROVIDER_ID: &str = "computesdk";
const DEFAULT_TIMEOUT_SECS: u32 = 900;

#[derive(Clone, Debug)]
struct HealthSnapshot {
    configured: bool,
    message: String,
    providers: Vec<String>,
}

pub struct ComputeSdkBridgeProvider {
    /// Base URL of the sidecar, e.g. `http://clotho-compute-sdk-bridge:8091`.
    bridge_url: Option<String>,
    http: reqwest::Client,
    /// Cached non-secret reason when unconfigured.
    reason: String,
    /// Last health probe (updated by live_descriptor / run_job).
    last_health: Mutex<Option<HealthSnapshot>>,
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
            last_health: Mutex::new(None),
        }
    }

    pub fn unconfigured() -> Self {
        Self {
            bridge_url: None,
            http: reqwest::Client::new(),
            reason:
                "ComputeSDK bridge not running — start with `just dev-compute-bridge` or set CLOTHO_COMPUTE_SDK_BRIDGE_URL"
                    .into(),
            last_health: Mutex::new(None),
        }
    }

    fn provider(err: impl std::fmt::Display) -> ComputeError {
        ComputeError::Provider(err.to_string())
    }

    /// Whether job credentials include at least one usable upstream key.
    fn job_has_upstream_credentials(spec: &JobSpec) -> bool {
        spec.provider_credentials
            .values()
            .any(|v| !v.trim().is_empty())
    }

    async fn probe_health(&self, base: &str) -> Option<HealthSnapshot> {
        let health_url = format!("{base}/health");
        let resp = self.http.get(&health_url).send().await.ok()?;
        let health: BridgeHealth = resp.json().await.ok()?;
        Some(HealthSnapshot {
            configured: health.configured,
            message: health.message,
            providers: health.providers,
        })
    }

    /// Forward all non-empty credentials as UPPER_SNAKE keys for the bridge
    /// catalog (any ComputeSDK upstream, not only E2B/Modal).
    fn credentials_for_bridge(spec: &JobSpec) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        for (k, v) in &spec.provider_credentials {
            if v.trim().is_empty() {
                continue;
            }
            let key = if k
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
            {
                k.clone()
            } else {
                // e2b_api_key → E2B_API_KEY
                k.to_uppercase()
            };
            m.insert(key, serde_json::Value::String(v.clone()));
        }
        m
    }

    fn descriptor_from_state(&self) -> ProviderDescriptor {
        let (configured, reason, notes) = match &self.bridge_url {
            None => (
                false,
                self.reason.clone(),
                "optional TypeScript sidecar wrapping ComputeSDK providers".to_string(),
            ),
            Some(url) => {
                let health = self.last_health.lock().ok().and_then(|g| g.clone());
                match health {
                    Some(h) if h.configured => (
                        true,
                        if h.providers.is_empty() {
                            String::new()
                        } else {
                            format!("upstream: {}", h.providers.join(", "))
                        },
                        format!(
                            "ComputeSDK bridge at {url}; upstream providers: {}",
                            h.providers.join(", ")
                        ),
                    ),
                    Some(h) => (
                        false,
                        if h.message.is_empty() {
                            "bridge reachable but no upstream provider credentials".into()
                        } else {
                            h.message
                        },
                        format!(
                            "ComputeSDK bridge at {url}; connect any ComputeSDK upstream keys in Clotho settings or on the bridge"
                        ),
                    ),
                    None => (
                        // Without a probe, do not claim configured — URL alone is not enough.
                        false,
                        "bridge URL set; upstream credentials not verified (connect ComputeSDK providers in settings)"
                            .into(),
                        format!(
                            "ComputeSDK bridge at {url}; supports all @computesdk/* providers when keys + packages present"
                        ),
                    ),
                }
            }
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
                gpu: false,
                gpu_types: vec![],
                cost_hints: "depends on upstream ComputeSDK provider".into(),
            },
            default_snapshot: String::new(),
            notes,
        }
    }
}

#[derive(Debug, Deserialize)]
struct BridgeHealth {
    #[serde(default)]
    configured: bool,
    #[serde(default)]
    message: String,
    #[serde(default)]
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
        self.descriptor_from_state()
    }

    async fn live_descriptor(&self) -> ProviderDescriptor {
        if let Some(base) = &self.bridge_url {
            let snap = match self.probe_health(base).await {
                Some(s) => s,
                None => HealthSnapshot {
                    configured: false,
                    message: "ComputeSDK bridge unreachable — start with `just dev-compute-bridge`"
                        .into(),
                    providers: vec![],
                },
            };
            if let Ok(mut g) = self.last_health.lock() {
                *g = Some(snap);
            }
        }
        self.descriptor_from_state()
    }

    async fn run_job(&self, spec: JobSpec) -> Result<JobResult, ComputeError> {
        let Some(base) = &self.bridge_url else {
            return Err(ComputeError::Disabled(self.reason.clone()));
        };
        if spec.commands.is_empty() {
            return Err(ComputeError::Invalid("no commands".into()));
        }

        let job_creds = Self::job_has_upstream_credentials(&spec);
        if let Some(snap) = self.probe_health(base).await {
            if let Ok(mut g) = self.last_health.lock() {
                *g = Some(snap.clone());
            }
            if !snap.configured && !job_creds {
                let msg = if snap.message.is_empty() {
                    "ComputeSDK bridge has no configured upstream providers — connect credentials in Clotho settings (any ComputeSDK provider)"
                        .into()
                } else {
                    snap.message
                };
                return Err(ComputeError::Disabled(msg));
            }
        } else if !job_creds {
            return Err(ComputeError::Disabled(
                "ComputeSDK bridge unreachable and no per-job upstream credentials".into(),
            ));
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

        let creds = Self::credentials_for_bridge(&spec);
        let mut body = serde_json::json!({
            "label": spec.label,
            "snapshot": spec.snapshot,
            "commands": spec.commands,
            "env": spec.env,
            "timeout_secs": timeout,
            "files": files,
        });
        if !creds.is_empty() {
            body["credentials"] = serde_json::Value::Object(creds);
        }

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
            // Map 503 (unconfigured) to Disabled for honest FAILED_PRECONDITION.
            if status.as_u16() == 503 {
                return Err(ComputeError::Disabled(format!("computesdk bridge: {text}")));
            }
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
        assert!(!p.live_descriptor().await.configured);
        let err = p
            .run_job(JobSpec {
                commands: vec!["true".into()],
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert!(matches!(err, ComputeError::Disabled(_)));
    }

    #[tokio::test]
    async fn url_only_does_not_claim_configured_without_health() {
        let p = ComputeSdkBridgeProvider::with_url("http://127.0.0.1:1");
        // No successful probe yet — must not lie.
        assert!(!p.descriptor().configured);
        assert!(p.descriptor().configured_reason.contains("upstream"));
    }
}
