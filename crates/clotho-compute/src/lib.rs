//! Clotho Compute Interface (CCI) — provider-agnostic sandbox execution
//! (vision spec §4.1, docs/prd.md §5 Stage 7/12, docs/adr/0008, docs/adr/0013).
//!
//! No-lock-in is a core product stance, so compute sits behind a swappable
//! [`ComputeProvider`] trait rather than a hardcoded vendor. Stage 12 adds a
//! capability-aware [`ProviderRegistry`] so callers can list providers, inspect
//! honest capability metadata, and route jobs by id or required features.
//!
//! The gRPC [`ComputeService`] wraps the registry; adding a new backend
//! (E2B, Modal, Box, a BYO-device runner, …) is another impl of the same trait
//! plus a registry registration, not a rewrite.
//!
//! The interface is deliberately generic and collaboration-agnostic: create a
//! sandbox from a snapshot, drop in files, run commands, return the exit code
//! and logs, tear down. It knows nothing about git or Forgejo — CI
//! orchestration (what to run, reporting status back to the PR) lives in the
//! api-gateway.

pub mod box_provider;
pub mod computesdk;
pub mod daytona;
pub mod registry;
pub mod service;

use std::collections::HashMap;

use async_trait::async_trait;

pub use box_provider::{BoxProvider, BoxStubProvider};
pub use computesdk::ComputeSdkBridgeProvider;
pub use daytona::DaytonaProvider;
pub use registry::ProviderRegistry;
pub use service::ComputeService;

/// A file staged into the sandbox before the job's commands run.
#[derive(Clone)]
pub struct JobFile {
    /// Absolute path inside the sandbox.
    pub path: String,
    pub content: Vec<u8>,
}

/// A one-shot job: launch a sandbox, place files, run commands in order.
#[derive(Clone, Default)]
pub struct JobSpec {
    /// Free-form label for logs/provenance (e.g. `<repo>@<short-sha>`).
    pub label: String,
    /// Provider snapshot/image; empty means the provider's configured default.
    pub snapshot: String,
    pub files: Vec<JobFile>,
    /// Shell commands, run in order; the job stops at the first non-zero exit.
    pub commands: Vec<String>,
    pub env: HashMap<String, String>,
    /// Per-command timeout in seconds; 0 means the provider default.
    pub timeout_secs: u32,
    /// Preferred registry provider id; empty lets the registry route.
    pub provider_id: String,
    /// Per-job credentials resolved by the api-gateway from Clotho secrets
    /// (docs/adr/0014). Common key: `api_key`. Prefer over process env.
    pub provider_credentials: HashMap<String, String>,
}

/// The result of a finished job (the sandbox is already torn down).
#[derive(Debug)]
pub struct JobResult {
    /// Exit code of the last command run (the first failing one, if any).
    pub exit_code: i32,
    /// Combined output across the job's commands.
    pub logs: String,
    /// Provider that ran the job (e.g. `daytona`).
    pub provider: String,
    /// Provider-side sandbox id, for cross-referencing in its dashboard.
    pub sandbox_id: String,
}

/// Honest capability flags for a provider (docs/adr/0013).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub one_shot_jobs: bool,
    pub persistent_workspaces: bool,
    pub snapshots: bool,
    pub templates: bool,
    pub regions: Vec<String>,
    pub ssh: bool,
    pub desktop: bool,
    pub public_url: bool,
    pub file_api: bool,
    pub terminal_streaming: bool,
    /// Free-text cost/tier hint when known; never a secret.
    pub cost_hints: String,
}

impl ProviderCapabilities {
    /// Stable string tags for APIs that want a flat list (web badges, SDK).
    pub fn feature_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        if self.one_shot_jobs {
            tags.push("one-shot-jobs".into());
        }
        if self.persistent_workspaces {
            tags.push("persistent-workspaces".into());
        }
        if self.snapshots {
            tags.push("snapshots".into());
        }
        if self.templates {
            tags.push("templates".into());
        }
        if self.ssh {
            tags.push("ssh".into());
        }
        if self.desktop {
            tags.push("desktop".into());
        }
        if self.public_url {
            tags.push("public-url".into());
        }
        if self.file_api {
            tags.push("file-api".into());
        }
        if self.terminal_streaming {
            tags.push("terminal-streaming".into());
        }
        if !self.regions.is_empty() {
            tags.push(format!("regions:{}", self.regions.join(",")));
        }
        tags
    }
}

/// Kind of provider implementation behind the CCI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    /// Direct Rust integration of a vendor API (e.g. Daytona).
    Direct,
    /// Optional bridge (e.g. ComputeSDK TypeScript sidecar).
    Bridge,
    /// Registered for design/routing but not fully integrated yet.
    Stub,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Bridge => "bridge",
            Self::Stub => "stub",
        }
    }
}

/// Non-secret provider state for registry / gRPC / REST (docs/adr/0013).
#[derive(Clone, Debug)]
pub struct ProviderDescriptor {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    /// Whether credentials/config are present so jobs can actually run.
    pub configured: bool,
    /// Why not configured, when applicable.
    pub configured_reason: String,
    pub capabilities: ProviderCapabilities,
    pub default_snapshot: String,
    pub notes: String,
}

impl ProviderDescriptor {
    pub fn feature_tags(&self) -> Vec<String> {
        self.capabilities.feature_tags()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ComputeError {
    /// No provider is configured (e.g. missing `DAYTONA_API_KEY`). The service
    /// stays up so the stack is healthy; jobs fail cleanly until a key is set.
    #[error("compute provider disabled: {0}")]
    Disabled(String),
    /// The caller's request was malformed.
    #[error("invalid job: {0}")]
    Invalid(String),
    /// The provider (or the network to it) failed.
    #[error("provider error: {0}")]
    Provider(String),
    /// Requested provider id is not registered.
    #[error("unknown compute provider: {0}")]
    NotFound(String),
}

/// The CCI: a thin, swappable abstraction over a sandbox-compute backend.
#[async_trait]
pub trait ComputeProvider: Send + Sync {
    /// Short provider name / registry id, e.g. `daytona`.
    fn name(&self) -> &str;

    /// Capability and configured-state metadata (no secrets).
    ///
    /// Prefer [`Self::live_descriptor`] for list/get when the provider can
    /// refresh honest configured state (e.g. probe a bridge health endpoint).
    fn descriptor(&self) -> ProviderDescriptor;

    /// Async refresh of configured state. Default: same as [`Self::descriptor`].
    async fn live_descriptor(&self) -> ProviderDescriptor {
        self.descriptor()
    }

    /// Run a job to completion in a fresh sandbox and tear it down.
    async fn run_job(&self, spec: JobSpec) -> Result<JobResult, ComputeError>;
}

/// A provider that always fails — used when no real provider is configured, so
/// the gRPC surface stays up and the stack is healthy without a credential.
pub struct DisabledProvider {
    id: String,
    display_name: String,
    reason: String,
    kind: ProviderKind,
    capabilities: ProviderCapabilities,
    notes: String,
}

impl DisabledProvider {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            id: "disabled".into(),
            display_name: "Disabled".into(),
            reason: reason.into(),
            kind: ProviderKind::Stub,
            capabilities: ProviderCapabilities::default(),
            notes: "placeholder when no real provider credential is configured".into(),
        }
    }

    pub fn with_id(id: impl Into<String>, reason: impl Into<String>) -> Self {
        let id = id.into();
        let display_name = match id.as_str() {
            "daytona" => "Daytona".into(),
            other => other.to_string(),
        };
        let capabilities = if id == "daytona" {
            ProviderCapabilities {
                one_shot_jobs: true,
                persistent_workspaces: true,
                snapshots: true,
                file_api: true,
                cost_hints: "Daytona cloud sandbox (API-key billed)".into(),
                ..Default::default()
            }
        } else {
            ProviderCapabilities::default()
        };
        Self {
            id,
            display_name,
            reason: reason.into(),
            kind: ProviderKind::Direct,
            capabilities,
            notes: "provider registered but credentials are not configured".into(),
        }
    }
}

#[async_trait]
impl ComputeProvider for DisabledProvider {
    fn name(&self) -> &str {
        &self.id
    }

    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: self.id.clone(),
            name: self.display_name.clone(),
            kind: self.kind,
            configured: false,
            configured_reason: self.reason.clone(),
            capabilities: self.capabilities.clone(),
            default_snapshot: String::new(),
            notes: self.notes.clone(),
        }
    }

    async fn run_job(&self, _spec: JobSpec) -> Result<JobResult, ComputeError> {
        Err(ComputeError::Disabled(self.reason.clone()))
    }
}

/// Build the Stage 12/14 registry from environment: always registers Daytona,
/// ComputeSDK bridge, and Box (each may be unconfigured until credentials
/// exist — env or per-job Clotho secrets).
pub fn registry_from_env() -> ProviderRegistry {
    let default_id = std::env::var("CLOTHO_COMPUTE_PROVIDER")
        .unwrap_or_else(|_| "daytona".to_string())
        .to_lowercase();

    let mut providers: Vec<std::sync::Arc<dyn ComputeProvider>> = Vec::new();

    // Explicit full disable.
    if matches!(default_id.as_str(), "disabled" | "none") {
        providers.push(std::sync::Arc::new(DisabledProvider::new(
            "compute explicitly disabled via CLOTHO_COMPUTE_PROVIDER",
        )));
        return ProviderRegistry::new(providers, "disabled");
    }

    // Always register Daytona so per-job credentials from Clotho secrets work
    // even when process env is empty (docs/adr/0014).
    providers.push(std::sync::Arc::new(DaytonaProvider::from_env_or_unconfigured()));

    // Optional ComputeSDK bridge (docs/adr/0013): always listed; configured
    // only when the bridge URL is set *and* upstream providers can accept jobs.
    if let Some(bridge) = ComputeSdkBridgeProvider::from_env() {
        providers.push(std::sync::Arc::new(bridge));
    } else {
        providers.push(std::sync::Arc::new(ComputeSdkBridgeProvider::unconfigured()));
    }

    // Box (Ascii) real client (Stage 14): always listed; per-job keys work
    // when process env is empty (docs/adr/0014).
    providers.push(std::sync::Arc::new(BoxProvider::from_env_or_unconfigured()));

    // If the operator named an unknown default, fall back to daytona for routing.
    let default = if providers.iter().any(|p| p.name() == default_id) {
        default_id
    } else {
        "daytona".into()
    };

    ProviderRegistry::new(providers, default)
}
