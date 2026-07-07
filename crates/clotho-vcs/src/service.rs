//! gRPC surface over the engine.

use clotho_common::pb::vcs::v1::{
    changed_file::ChangeKind,
    vcs_server::{Vcs, VcsServer},
    ChangedFile, CheckpointRequest, CheckpointResponse, CommitRequest, CommitResponse,
    CommitSummary, DiffCommitsRequest, DiffCommitsResponse, FileEntry, GetHeadsRequest,
    GetHeadsResponse, InitRepoRequest, InitRepoResponse, ListFilesRequest, ListFilesResponse,
    OpLogEntry, QueryOpLogRequest, QueryOpLogResponse, RestoreToRequest, RestoreToResponse,
};
use tonic::{Request, Response, Status};

use crate::engine::{self, CommitParams, EngineError, FileChange, VcsEngine};

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

    async fn get_heads(
        &self,
        request: Request<GetHeadsRequest>,
    ) -> Result<Response<GetHeadsResponse>, Status> {
        let repo = request.into_inner().repo;
        let heads = self
            .engine
            .run(move |engine| async move { engine.get_heads(&repo).await })
            .await?;
        Ok(Response::new(GetHeadsResponse {
            heads: heads.heads.into_iter().map(commit_summary).collect(),
            main_commit_id: heads.main_commit_id.unwrap_or_default(),
        }))
    }

    async fn list_files(
        &self,
        request: Request<ListFilesRequest>,
    ) -> Result<Response<ListFilesResponse>, Status> {
        let req = request.into_inner();
        let (repo, commit_id) = (req.repo, req.commit_id);
        let list = self
            .engine
            .run(move |engine| async move {
                let commit_id = (!commit_id.is_empty()).then_some(commit_id.as_str());
                engine.list_files(&repo, commit_id).await
            })
            .await?;
        Ok(Response::new(ListFilesResponse {
            commit_id: list.commit_id,
            files: list
                .files
                .into_iter()
                .map(|f| FileEntry {
                    path: f.path,
                    size_bytes: f.size_bytes,
                    executable: f.executable,
                })
                .collect(),
        }))
    }

    async fn diff_commits(
        &self,
        request: Request<DiffCommitsRequest>,
    ) -> Result<Response<DiffCommitsResponse>, Status> {
        let req = request.into_inner();
        let (repo, from, to) = (req.repo, req.from_commit_id, req.to_commit_id);
        if to.is_empty() {
            return Err(Status::invalid_argument("to_commit_id is required"));
        }
        let diff = self
            .engine
            .run(move |engine| async move {
                let from = (!from.is_empty()).then_some(from.as_str());
                engine.diff_commits(&repo, from, &to).await
            })
            .await?;
        Ok(Response::new(DiffCommitsResponse {
            from_commit_id: diff.from_commit_id,
            to_commit_id: diff.to_commit_id,
            files: diff
                .files
                .into_iter()
                .map(|f| ChangedFile {
                    path: f.path,
                    kind: match f.kind {
                        engine::ChangeKind::Added => ChangeKind::Added,
                        engine::ChangeKind::Modified => ChangeKind::Modified,
                        engine::ChangeKind::Deleted => ChangeKind::Deleted,
                    } as i32,
                    old_content: f.old_content,
                    new_content: f.new_content,
                })
                .collect(),
        }))
    }
}

fn commit_summary(c: engine::CommitSummary) -> CommitSummary {
    CommitSummary {
        commit_id: c.commit_id,
        change_id: c.change_id,
        description: c.description,
        author_name: c.author_name,
        author_email: c.author_email,
        timestamp_millis: c.timestamp_millis,
        parent_commit_ids: c.parent_commit_ids,
    }
}
