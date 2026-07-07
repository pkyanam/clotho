//! gRPC surface over the engine.

use clotho_common::pb::vcs::v1::{
    vcs_server::{Vcs, VcsServer},
    CheckpointRequest, CheckpointResponse, CommitRequest, CommitResponse, InitRepoRequest,
    InitRepoResponse, OpLogEntry, QueryOpLogRequest, QueryOpLogResponse, RestoreToRequest,
    RestoreToResponse,
};
use tonic::{Request, Response, Status};

use crate::engine::{CommitParams, EngineError, FileChange, VcsEngine};

pub struct VcsService {
    engine: VcsEngine,
}

impl VcsService {
    pub fn new(engine: VcsEngine) -> Self {
        Self { engine }
    }

    pub fn into_server(self) -> VcsServer<Self> {
        VcsServer::new(self)
    }
}

impl From<EngineError> for Status {
    fn from(err: EngineError) -> Self {
        match &err {
            EngineError::InvalidRepoName(_)
            | EngineError::InvalidId(_)
            | EngineError::InvalidPath(..) => Status::invalid_argument(err.to_string()),
            EngineError::RepoExists(_) => Status::already_exists(err.to_string()),
            EngineError::RepoNotFound(_) => Status::not_found(err.to_string()),
            EngineError::Other(_) => Status::internal(err.to_string()),
        }
    }
}

#[tonic::async_trait]
impl Vcs for VcsService {
    async fn init_repo(
        &self,
        request: Request<InitRepoRequest>,
    ) -> Result<Response<InitRepoResponse>, Status> {
        let req = request.into_inner();
        let name = req.name.clone();
        let operation_id = self
            .engine
            .run(move |engine| async move { engine.init_repo(&name).await })
            .await?;
        tracing::info!(repo = %req.name, "repo initialized");
        Ok(Response::new(InitRepoResponse {
            name: req.name,
            operation_id,
        }))
    }

    async fn commit(
        &self,
        request: Request<CommitRequest>,
    ) -> Result<Response<CommitResponse>, Status> {
        let req = request.into_inner();
        let repo = req.repo.clone();
        let params = CommitParams {
            parent_commit_ids: req.parent_commit_ids,
            files: req
                .files
                .into_iter()
                .map(|f| FileChange {
                    path: f.path,
                    content: f.content,
                    executable: f.executable,
                })
                .collect(),
            deleted_paths: req.deleted_paths,
            message: req.message,
            author_name: req.author_name,
            author_email: req.author_email,
        };
        let outcome = self
            .engine
            .run(move |engine| async move { engine.commit(&repo, params).await })
            .await?;
        tracing::info!(repo = %req.repo, commit = %outcome.commit_id, "commit written");
        Ok(Response::new(CommitResponse {
            commit_id: outcome.commit_id,
            change_id: outcome.change_id,
            operation_id: outcome.operation_id,
        }))
    }

    async fn checkpoint(
        &self,
        request: Request<CheckpointRequest>,
    ) -> Result<Response<CheckpointResponse>, Status> {
        let req = request.into_inner();
        let (repo, label) = (req.repo.clone(), req.label.clone());
        let operation_id = self
            .engine
            .run(move |engine| async move { engine.checkpoint(&repo, &label).await })
            .await?;
        tracing::info!(repo = %req.repo, op = %operation_id, "checkpoint recorded");
        Ok(Response::new(CheckpointResponse { operation_id }))
    }

    async fn restore_to(
        &self,
        request: Request<RestoreToRequest>,
    ) -> Result<Response<RestoreToResponse>, Status> {
        let req = request.into_inner();
        let (repo, op) = (req.repo.clone(), req.operation_id.clone());
        let operation_id = self
            .engine
            .run(move |engine| async move { engine.restore_to(&repo, &op).await })
            .await?;
        tracing::info!(repo = %req.repo, op = %operation_id, "restored");
        Ok(Response::new(RestoreToResponse { operation_id }))
    }

    async fn query_op_log(
        &self,
        request: Request<QueryOpLogRequest>,
    ) -> Result<Response<QueryOpLogResponse>, Status> {
        let req = request.into_inner();
        let (repo, limit) = (req.repo, req.limit);
        let entries = self
            .engine
            .run(move |engine| async move { engine.query_op_log(&repo, limit).await })
            .await?;
        Ok(Response::new(QueryOpLogResponse {
            entries: entries
                .into_iter()
                .map(|e| OpLogEntry {
                    operation_id: e.operation_id,
                    description: e.description,
                    start_time_millis: e.start_time_millis,
                    end_time_millis: e.end_time_millis,
                    parent_operation_ids: e.parent_operation_ids,
                })
                .collect(),
        }))
    }
}
