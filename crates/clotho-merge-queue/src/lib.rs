//! The merge-queue: multi-agent reconciliation, naive-but-real (docs/prd.md
//! §5 Stage 5).
//!
//! Agents commit concurrently on their own heads; nothing blocks at write
//! time. Landing on the shared `main` history goes through this queue, which
//! serializes integrations per repository and delegates the actual
//! fast-forward/rebase to clotho-vcs (`IntegrateCommit`) — the engine owns
//! repo mutation, the queue owns ordering. Conflicts never block the queue:
//! a conflicted rebase lands as a first-class jj conflict commit and `main`
//! still advances (vision spec §3.1).
//!
//! Deliberately naive for the prototype (per §7: *a* working answer, not
//! *the* answer): one in-process queue per repo via an async mutex — no
//! persistence, no batching, no speculative CI. Those come later.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use clotho_common::pb::mergequeue::v1::{
    merge_queue_server::{MergeQueue, MergeQueueServer},
    SubmitChangeRequest, SubmitChangeResponse,
};
use clotho_common::pb::vcs::v1::vcs_client::VcsClient;
use clotho_common::pb::vcs::v1::IntegrateCommitRequest;
use tonic::transport::Channel;
use tonic::{Request, Response, Status};

pub struct MergeQueueService {
    vcs: VcsClient<Channel>,
    /// One lock per repository: integrations are strictly serialized per
    /// repo, fully concurrent across repos.
    repo_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl MergeQueueService {
    pub fn new(vcs_grpc_url: &str) -> Result<Self, clotho_common::Error> {
        // Lazy channel: connects on first use and reconnects on failure, so
        // the queue starts cleanly regardless of service start order.
        let channel = Channel::from_shared(vcs_grpc_url.to_string())
            .map_err(|e| clotho_common::Error::Config(format!("vcs url {vcs_grpc_url:?}: {e}")))?
            .connect_lazy();
        Ok(Self {
            vcs: VcsClient::new(channel),
            repo_locks: Mutex::new(HashMap::new()),
        })
    }

    pub fn into_server(self) -> MergeQueueServer<Self> {
        MergeQueueServer::new(self)
    }

    fn repo_lock(&self, repo: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.repo_locks.lock().expect("repo_locks poisoned");
        locks.entry(repo.to_string()).or_default().clone()
    }
}

#[tonic::async_trait]
impl MergeQueue for MergeQueueService {
    async fn submit_change(
        &self,
        request: Request<SubmitChangeRequest>,
    ) -> Result<Response<SubmitChangeResponse>, Status> {
        let req = request.into_inner();
        if req.repo.is_empty() || req.commit_id.is_empty() {
            return Err(Status::invalid_argument("repo and commit_id are required"));
        }

        // Take this repo's turn. Everything after the await runs alone for
        // this repo, so the read-modify-write inside IntegrateCommit can't
        // interleave with another submission.
        let lock = self.repo_lock(&req.repo);
        let _turn = lock.lock().await;

        let outcome = self
            .vcs
            .clone()
            .integrate_commit(IntegrateCommitRequest {
                repo: req.repo.clone(),
                commit_id: req.commit_id.clone(),
            })
            .await?
            .into_inner();
        tracing::info!(
            repo = %req.repo,
            submitted = %req.commit_id,
            landed = %outcome.commit_id,
            fast_forwarded = outcome.fast_forwarded,
            conflicted = outcome.conflicted,
            "change integrated"
        );
        Ok(Response::new(SubmitChangeResponse {
            commit_id: outcome.commit_id,
            change_id: outcome.change_id,
            operation_id: outcome.operation_id,
            fast_forwarded: outcome.fast_forwarded,
            conflicted: outcome.conflicted,
            conflicted_paths: outcome.conflicted_paths,
        }))
    }
}
