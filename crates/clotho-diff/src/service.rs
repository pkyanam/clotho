//! gRPC surface over the diff engine.

use clotho_common::pb::diff::v1::{
    diff_server::{Diff, DiffServer},
    ChangeStatus, DiffFilesRequest, DiffFilesResponse, FileDiff, SymbolChange,
};
use tonic::{Request, Response, Status};

use crate::engine::{self, DiffError};

pub struct DiffService;

impl DiffService {
    pub fn into_server(self) -> DiffServer<Self> {
        DiffServer::new(self)
    }
}

impl From<DiffError> for Status {
    fn from(err: DiffError) -> Self {
        match &err {
            DiffError::NotUtf8(..) => Status::invalid_argument(err.to_string()),
            DiffError::Parser(_) => Status::internal(err.to_string()),
        }
    }
}

fn status(s: engine::ChangeStatus) -> ChangeStatus {
    match s {
        engine::ChangeStatus::Added => ChangeStatus::Added,
        engine::ChangeStatus::Modified => ChangeStatus::Modified,
        engine::ChangeStatus::Removed => ChangeStatus::Removed,
    }
}

#[tonic::async_trait]
impl Diff for DiffService {
    async fn diff_files(
        &self,
        request: Request<DiffFilesRequest>,
    ) -> Result<Response<DiffFilesResponse>, Status> {
        let req = request.into_inner();
        // Parsing is CPU-bound; keep it off the async runtime's worker pool.
        let files = tokio::task::spawn_blocking(move || {
            req.files
                .iter()
                .map(|f| engine::diff_file(&f.path, &f.old_content, &f.new_content))
                .collect::<Result<Vec<_>, _>>()
        })
        .await
        .map_err(|e| Status::internal(format!("diff task join error: {e}")))??;

        Ok(Response::new(DiffFilesResponse {
            files: files
                .into_iter()
                .map(|f| FileDiff {
                    path: f.path,
                    language: f.language.unwrap_or_default().to_string(),
                    status: status(f.status) as i32,
                    symbols: f
                        .symbols
                        .into_iter()
                        .map(|s| SymbolChange {
                            name: s.name,
                            kind: s.kind,
                            status: status(s.status) as i32,
                            old_start_line: s.old_lines.map_or(0, |(s, _)| s),
                            old_end_line: s.old_lines.map_or(0, |(_, e)| e),
                            new_start_line: s.new_lines.map_or(0, |(s, _)| s),
                            new_end_line: s.new_lines.map_or(0, |(_, e)| e),
                        })
                        .collect(),
                })
                .collect(),
        }))
    }
}
