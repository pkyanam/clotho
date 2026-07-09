//! Multi-provider registry for the CCI (docs/adr/0013).

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    ComputeError, ComputeProvider, JobResult, JobSpec, ProviderCapabilities, ProviderDescriptor,
};

/// Holds every registered [`ComputeProvider`] and routes jobs by id or
/// required capabilities.
pub struct ProviderRegistry {
    providers: Vec<Arc<dyn ComputeProvider>>,
    default_id: String,
}

impl ProviderRegistry {
    pub fn new(providers: Vec<Arc<dyn ComputeProvider>>, default_id: impl Into<String>) -> Self {
        Self {
            providers,
            default_id: default_id.into(),
        }
    }

    pub fn default_id(&self) -> &str {
        &self.default_id
    }

    pub fn list(&self) -> Vec<ProviderDescriptor> {
        self.providers.iter().map(|p| p.descriptor()).collect()
    }

    /// Descriptors with `enabled` semantics: the default id is enabled when
    /// it exists in the registry (whether or not it is configured).
    pub fn list_infos(&self) -> Vec<(ProviderDescriptor, bool)> {
        self.providers
            .iter()
            .map(|p| {
                let d = p.descriptor();
                let enabled = d.id == self.default_id;
                (d, enabled)
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<ProviderDescriptor> {
        let id = id.to_lowercase();
        self.providers
            .iter()
            .find(|p| p.name() == id)
            .map(|p| p.descriptor())
    }

    fn provider_by_id(&self, id: &str) -> Option<Arc<dyn ComputeProvider>> {
        let id = id.to_lowercase();
        self.providers.iter().find(|p| p.name() == id).cloned()
    }

    /// Resolve which provider should run a job.
    ///
    /// 1. Explicit `spec.provider_id` if registered.
    /// 2. Else default id if configured and supports one-shot jobs.
    /// 3. Else first configured provider that supports one-shot jobs.
    /// 4. Else default id even if unconfigured (so callers get a clean
    ///    Disabled/FAILED_PRECONDITION rather than NotFound).
    pub fn resolve(&self, provider_id: &str) -> Result<Arc<dyn ComputeProvider>, ComputeError> {
        if !provider_id.is_empty() {
            return self
                .provider_by_id(provider_id)
                .ok_or_else(|| ComputeError::NotFound(provider_id.to_string()));
        }

        if let Some(p) = self.provider_by_id(&self.default_id) {
            let d = p.descriptor();
            if d.configured && d.capabilities.one_shot_jobs {
                return Ok(p);
            }
        }

        if let Some(p) = self.providers.iter().find(|p| {
            let d = p.descriptor();
            d.configured && d.capabilities.one_shot_jobs
        }) {
            return Ok(p.clone());
        }

        // Prefer the named default for a predictable error message.
        if let Some(p) = self.provider_by_id(&self.default_id) {
            return Ok(p);
        }

        self.providers
            .first()
            .cloned()
            .ok_or_else(|| ComputeError::Disabled("no compute providers registered".into()))
    }

    /// Providers that match all required capability flags.
    pub fn matching(&self, required: &ProviderCapabilities) -> Vec<ProviderDescriptor> {
        self.providers
            .iter()
            .map(|p| p.descriptor())
            .filter(|d| d.configured && capabilities_match(&d.capabilities, required))
            .collect()
    }
}

fn capabilities_match(have: &ProviderCapabilities, need: &ProviderCapabilities) -> bool {
    (!need.one_shot_jobs || have.one_shot_jobs)
        && (!need.persistent_workspaces || have.persistent_workspaces)
        && (!need.snapshots || have.snapshots)
        && (!need.templates || have.templates)
        && (!need.ssh || have.ssh)
        && (!need.desktop || have.desktop)
        && (!need.public_url || have.public_url)
        && (!need.file_api || have.file_api)
        && (!need.terminal_streaming || have.terminal_streaming)
        && need
            .regions
            .iter()
            .all(|r| have.regions.iter().any(|h| h.eq_ignore_ascii_case(r)))
}

/// Registry also implements [`ComputeProvider`] so existing single-provider
/// call sites can treat it as "the" compute backend: `name` is the default id.
#[async_trait]
impl ComputeProvider for ProviderRegistry {
    fn name(&self) -> &str {
        &self.default_id
    }

    fn descriptor(&self) -> ProviderDescriptor {
        self.provider_by_id(&self.default_id)
            .map(|p| p.descriptor())
            .unwrap_or_else(|| ProviderDescriptor {
                id: self.default_id.clone(),
                name: "Registry".into(),
                kind: crate::ProviderKind::Stub,
                configured: false,
                configured_reason: "default provider missing".into(),
                capabilities: ProviderCapabilities::default(),
                default_snapshot: String::new(),
                notes: String::new(),
            })
    }

    async fn run_job(&self, mut spec: JobSpec) -> Result<JobResult, ComputeError> {
        let provider_id = std::mem::take(&mut spec.provider_id);
        let provider = self.resolve(&provider_id)?;
        provider.run_job(spec).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DisabledProvider, ProviderKind};

    struct FakeConfigured {
        id: &'static str,
        one_shot: bool,
    }

    #[async_trait]
    impl ComputeProvider for FakeConfigured {
        fn name(&self) -> &str {
            self.id
        }

        fn descriptor(&self) -> ProviderDescriptor {
            ProviderDescriptor {
                id: self.id.into(),
                name: self.id.into(),
                kind: ProviderKind::Direct,
                configured: true,
                configured_reason: String::new(),
                capabilities: ProviderCapabilities {
                    one_shot_jobs: self.one_shot,
                    ..Default::default()
                },
                default_snapshot: "img".into(),
                notes: String::new(),
            }
        }

        async fn run_job(&self, _spec: JobSpec) -> Result<JobResult, ComputeError> {
            Ok(JobResult {
                exit_code: 0,
                logs: format!("ran on {}", self.id),
                provider: self.id.into(),
                sandbox_id: "sb".into(),
            })
        }
    }

    #[tokio::test]
    async fn routes_explicit_provider_id() {
        let reg = ProviderRegistry::new(
            vec![
                Arc::new(FakeConfigured {
                    id: "daytona",
                    one_shot: true,
                }),
                Arc::new(FakeConfigured {
                    id: "computesdk",
                    one_shot: true,
                }),
            ],
            "daytona",
        );
        let result = reg
            .run_job(JobSpec {
                provider_id: "computesdk".into(),
                commands: vec!["true".into()],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(result.provider, "computesdk");
    }

    #[tokio::test]
    async fn falls_back_to_first_configured_one_shot() {
        let reg = ProviderRegistry::new(
            vec![
                Arc::new(DisabledProvider::with_id("daytona", "no key")),
                Arc::new(FakeConfigured {
                    id: "computesdk",
                    one_shot: true,
                }),
            ],
            "daytona",
        );
        let result = reg
            .run_job(JobSpec {
                commands: vec!["true".into()],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(result.provider, "computesdk");
    }

    #[tokio::test]
    async fn unknown_provider_is_not_found() {
        let reg = ProviderRegistry::new(
            vec![Arc::new(FakeConfigured {
                id: "daytona",
                one_shot: true,
            })],
            "daytona",
        );
        let err = reg
            .run_job(JobSpec {
                provider_id: "nope".into(),
                commands: vec!["true".into()],
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert!(matches!(err, ComputeError::NotFound(_)));
    }
}
