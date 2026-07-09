//! gRPC surface over a [`ProviderRegistry`] (the CCI, docs/adr/0013).

use std::sync::Arc;

use clotho_common::pb::compute::v1::{
    compute_server::{Compute, ComputeServer},
    GetProviderRequest, GetProviderResponse, ListProvidersRequest, ListProvidersResponse,
    ProviderCapabilities as PbCapabilities, ProviderInfo, RunJobRequest, RunJobResponse,
};
use tonic::{Request, Response, Status};

use crate::{
    ComputeError, ComputeProvider, JobFile, JobSpec, ProviderDescriptor, ProviderRegistry,
};

pub struct ComputeService {
    registry: Arc<ProviderRegistry>,
}

impl ComputeService {
    pub fn new(registry: Arc<ProviderRegistry>) -> Self {
        Self { registry }
    }

    pub fn into_server(self) -> ComputeServer<Self> {
        ComputeServer::new(self)
    }
}

impl From<ComputeError> for Status {
    fn from(err: ComputeError) -> Self {
        match &err {
            ComputeError::Disabled(_) => Status::failed_precondition(err.to_string()),
            ComputeError::Invalid(_) => Status::invalid_argument(err.to_string()),
            ComputeError::Provider(_) => Status::unavailable(err.to_string()),
            ComputeError::NotFound(_) => Status::not_found(err.to_string()),
        }
    }
}

fn to_pb_info(d: ProviderDescriptor, enabled: bool) -> ProviderInfo {
    let caps = d.capabilities;
    ProviderInfo {
        id: d.id,
        name: d.name,
        kind: d.kind.as_str().to_string(),
        enabled,
        configured: d.configured,
        configured_reason: d.configured_reason,
        capabilities: Some(PbCapabilities {
            one_shot_jobs: caps.one_shot_jobs,
            persistent_workspaces: caps.persistent_workspaces,
            snapshots: caps.snapshots,
            templates: caps.templates,
            regions: caps.regions,
            ssh: caps.ssh,
            desktop: caps.desktop,
            public_url: caps.public_url,
            file_api: caps.file_api,
            terminal_streaming: caps.terminal_streaming,
            cost_hints: caps.cost_hints,
        }),
        default_snapshot: d.default_snapshot,
        notes: d.notes,
    }
}

#[tonic::async_trait]
impl Compute for ComputeService {
    async fn run_job(
        &self,
        request: Request<RunJobRequest>,
    ) -> Result<Response<RunJobResponse>, Status> {
        let req = request.into_inner();
        if req.commands.is_empty() {
            return Err(Status::invalid_argument("at least one command is required"));
        }
        let spec = JobSpec {
            label: req.label,
            snapshot: req.snapshot,
            files: req
                .files
                .into_iter()
                .map(|f| JobFile {
                    path: f.path,
                    content: f.content,
                })
                .collect(),
            commands: req.commands,
            env: req.env,
            timeout_secs: req.timeout_secs,
            provider_id: req.provider_id,
        };
        let label = spec.label.clone();
        let requested = spec.provider_id.clone();
        tracing::info!(
            default = self.registry.default_id(),
            requested = %requested,
            job = %label,
            "running compute job"
        );
        let result = self.registry.run_job(spec).await?;
        tracing::info!(
            provider = %result.provider,
            job = %label,
            sandbox = %result.sandbox_id,
            exit_code = result.exit_code,
            "compute job finished"
        );
        Ok(Response::new(RunJobResponse {
            exit_code: result.exit_code,
            logs: result.logs,
            provider: result.provider,
            sandbox_id: result.sandbox_id,
        }))
    }

    async fn list_providers(
        &self,
        _request: Request<ListProvidersRequest>,
    ) -> Result<Response<ListProvidersResponse>, Status> {
        let providers = self
            .registry
            .list_infos()
            .into_iter()
            .map(|(d, enabled)| to_pb_info(d, enabled))
            .collect();
        Ok(Response::new(ListProvidersResponse {
            providers,
            default_provider_id: self.registry.default_id().to_string(),
        }))
    }

    async fn get_provider(
        &self,
        request: Request<GetProviderRequest>,
    ) -> Result<Response<GetProviderResponse>, Status> {
        let id = request.into_inner().provider_id;
        if id.trim().is_empty() {
            return Err(Status::invalid_argument("provider_id is required"));
        }
        let Some(d) = self.registry.get(&id) else {
            return Err(Status::not_found(format!(
                "compute provider {id:?} not found"
            )));
        };
        let enabled = d.id == self.registry.default_id();
        Ok(Response::new(GetProviderResponse {
            provider: Some(to_pb_info(d, enabled)),
        }))
    }
}
