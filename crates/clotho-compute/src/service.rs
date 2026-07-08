//! gRPC surface over a [`ComputeProvider`] (the CCI).

use std::sync::Arc;

use clotho_common::pb::compute::v1::{
    compute_server::{Compute, ComputeServer},
    RunJobRequest, RunJobResponse,
};
use tonic::{Request, Response, Status};

use crate::{ComputeError, ComputeProvider, JobFile, JobSpec};

pub struct ComputeService {
    provider: Arc<dyn ComputeProvider>,
}

impl ComputeService {
    pub fn new(provider: Arc<dyn ComputeProvider>) -> Self {
        Self { provider }
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
        }
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
        };
        let label = spec.label.clone();
        tracing::info!(provider = self.provider.name(), job = %label, "running compute job");
        let result = self.provider.run_job(spec).await?;
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
}
