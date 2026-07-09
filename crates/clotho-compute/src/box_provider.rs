//! Box provider stub — persistent agent workspace / long-running sandbox.
//!
//! Stage 12 registers Box in the CCI registry with honest capability metadata
//! drawn from the public Box API (https://docs.ascii.dev/llms.txt,
//! https://docs.ascii.dev/box/api/v1):
//!
//! - Base URL: `https://ascii.dev/api/box/v1`
//! - Auth: `Authorization: Bearer $BOX_API_KEY` (`box_...` service keys)
//! - Lifecycle: create / get / stop(archive) / resume / fork / delete
//! - Work: prompt (codex | claude-code), events (long-poll stream), interrupt
//! - Access: SSH key inject, desktop/noVNC streaming, public hosting subdomain
//! - Files: read/write paths; command exec; artifacts; snapshot tree/download
//!
//! Full HTTP integration is deferred; without credentials (or until the client
//! lands) the provider is unconfigured and jobs fail cleanly with `Disabled`
//! (docs/adr/0013, docs/prd.md Stage 12).

use async_trait::async_trait;

use crate::{
    ComputeError, ComputeProvider, JobResult, JobSpec, ProviderCapabilities, ProviderDescriptor,
    ProviderKind,
};

const PROVIDER_ID: &str = "box";
/// Public Box API v1 base (docs.ascii.dev/box/api/v1).
const DEFAULT_API_URL: &str = "https://ascii.dev/api/box/v1";

pub struct BoxStubProvider {
    configured: bool,
    configured_reason: String,
    api_url: String,
}

impl BoxStubProvider {
    /// Always registers; configured only when a real client exists.
    ///
    /// `BOX_API_KEY` + optional `BOX_API_URL` are the documented credential
    /// surface. Stage 12 still does not call the API — presence of a key is
    /// recorded in notes only so operators know the env is ready for a later
    /// adapter.
    pub fn from_env() -> Self {
        let api_url = std::env::var("BOX_API_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_API_URL.into());
        match std::env::var("BOX_API_KEY") {
            Ok(key) if !key.trim().is_empty() => Self {
                configured: false,
                configured_reason:
                    "Box adapter is a Stage 12 stub; BOX_API_KEY is set but the HTTP client is not implemented yet"
                        .into(),
                api_url,
            },
            _ => Self {
                configured: false,
                configured_reason: "BOX_API_KEY not set; Box persistent workspaces unavailable"
                    .into(),
                api_url,
            },
        }
    }

    pub fn unconfigured() -> Self {
        Self {
            configured: false,
            configured_reason: "BOX_API_KEY not set".into(),
            api_url: DEFAULT_API_URL.into(),
        }
    }
}

#[async_trait]
impl ComputeProvider for BoxStubProvider {
    fn name(&self) -> &str {
        PROVIDER_ID
    }

    fn descriptor(&self) -> ProviderDescriptor {
        // Capabilities match the documented v1 surface:
        // POST /boxes, POST /commands, PUT/GET /files, POST /desktop,
        // POST /sshkey, snapshots, public subdomain hosting, event streaming.
        // one_shot_jobs is true because create → files → commands → delete is
        // a valid path; primary product model is still persistent workspaces.
        ProviderDescriptor {
            id: PROVIDER_ID.into(),
            name: "Box".into(),
            kind: ProviderKind::Stub,
            configured: self.configured,
            configured_reason: self.configured_reason.clone(),
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
                cost_hints: "persistent Ubuntu VM; TTL / pay-per-use (see Box dashboard)".into(),
            },
            default_snapshot: String::new(),
            notes: format!(
                "stub for ascii Box API v1 at {url}; auth BOX_API_KEY (box_…); \
                 lifecycle create/stop/resume/fork, prompt, events, desktop, SSH, files, commands, snapshots",
                url = self.api_url
            ),
        }
    }

    async fn run_job(&self, _spec: JobSpec) -> Result<JobResult, ComputeError> {
        Err(ComputeError::Disabled(self.configured_reason.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_advertises_box_v1_caps_and_rejects_jobs() {
        let p = BoxStubProvider::unconfigured();
        let d = p.descriptor();
        assert_eq!(d.id, "box");
        assert!(!d.configured);
        assert!(d.capabilities.persistent_workspaces);
        assert!(d.capabilities.ssh);
        assert!(d.capabilities.desktop);
        assert!(d.capabilities.public_url);
        assert!(d.capabilities.file_api);
        assert!(d.capabilities.one_shot_jobs);
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
}
