//! Clotho Compute Interface (CCI) — provider-agnostic sandbox execution
//! (vision spec §4.1, docs/prd.md §5 Stage 7, docs/adr/0008).
//!
//! No-lock-in is a core product stance, so compute sits behind a swappable
//! [`ComputeProvider`] trait rather than a hardcoded vendor. The gRPC
//! [`ComputeService`] wraps whichever provider is configured; adding a new
//! backend (E2B, Modal, a BYO-device runner, …) is another impl of the same
//! trait, not a rewrite.
//!
//! The interface is deliberately generic and collaboration-agnostic: create a
//! sandbox from a snapshot, drop in files, run commands, return the exit code
//! and logs, tear down. It knows nothing about git or Forgejo — CI
//! orchestration (what to run, reporting status back to the PR) lives in the
//! api-gateway.

pub mod daytona;
pub mod service;

use std::collections::HashMap;

use async_trait::async_trait;

pub use daytona::DaytonaProvider;
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
}

/// The result of a finished job (the sandbox is already torn down).
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
}

/// The CCI: a thin, swappable abstraction over a sandbox-compute backend.
#[async_trait]
pub trait ComputeProvider: Send + Sync {
    /// Short provider name for logs/provenance, e.g. `daytona`.
    fn name(&self) -> &str;

    /// Run a job to completion in a fresh sandbox and tear it down.
    async fn run_job(&self, spec: JobSpec) -> Result<JobResult, ComputeError>;
}

/// A provider that always fails — used when no real provider is configured, so
/// the gRPC surface stays up and the stack is healthy without a credential.
pub struct DisabledProvider {
    reason: String,
}

impl DisabledProvider {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl ComputeProvider for DisabledProvider {
    fn name(&self) -> &str {
        "disabled"
    }

    async fn run_job(&self, _spec: JobSpec) -> Result<JobResult, ComputeError> {
        Err(ComputeError::Disabled(self.reason.clone()))
    }
}
