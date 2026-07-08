//! Edge API gateway — Clotho's REST/JSON boundary (docs/prd.md §2).
//!
//! Aggregates the internal gRPC services and Forgejo's REST API behind one
//! boring, debuggable HTTP surface. Stage 3 landed the first real endpoint:
//! `POST /api/v1/repos` provisions a clotho-vcs repository (jj-native, real
//! git objects) *and* its Forgejo project in a single call (docs/adr/0003).
//! Stage 6 grows the read side the web app browses (docs/adr/0007): repo
//! tree/files/commits/op log from clotho-vcs, PRs proxied from Forgejo, the
//! structured PR diff composed from clotho-vcs + clotho-diff, and agent
//! presence proxied from the agent gateway's audit log.

mod agents;
mod ci;
pub mod error;
pub mod forgejo;
mod issues;
mod pulls;
mod repos;
mod status;
mod webhooks;

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use clotho_common::pb::compute::v1::compute_client::ComputeClient;
use clotho_common::pb::diff::v1::diff_client::DiffClient;
use clotho_common::pb::mergequeue::v1::merge_queue_client::MergeQueueClient;
use clotho_common::pb::mergequeue::v1::SubmitChangeRequest;
use clotho_common::pb::vcs::v1::vcs_client::VcsClient;
use clotho_common::pb::vcs::v1::{CommitRequest, FileChange, InitRepoRequest};
use serde::{Deserialize, Serialize};
use tonic::transport::Channel;
use tower_http::cors::CorsLayer;

use crate::error::ApiError;
use crate::forgejo::{ForgejoClient, ForgejoConfig, RepoInfo};

#[derive(Clone)]
pub struct GatewayConfig {
    /// clotho-vcs gRPC endpoint, e.g. `http://clotho-vcs:50051`.
    pub vcs_grpc_url: String,
    /// clotho-diff gRPC endpoint, e.g. `http://clotho-diff:50055`.
    pub diff_grpc_url: String,
    /// clotho-merge-queue gRPC endpoint, e.g. `http://clotho-merge-queue:50053`.
    pub merge_queue_grpc_url: String,
    /// clotho-agent-gateway admin HTTP base, e.g. `http://clotho-agent-gateway:8090`.
    pub agent_gateway_url: String,
    /// Admin bearer token for the agent gateway (service-to-service).
    pub agent_admin_token: String,
    /// clotho-compute gRPC endpoint, e.g. `http://clotho-compute:50057`.
    pub compute_grpc_url: String,
    /// Shared secret Forgejo signs push webhooks with (HMAC-SHA256). Empty
    /// disables signature verification (dev only).
    pub webhook_secret: String,
    /// URL Forgejo should call for push events; registered per repo at
    /// creation. Empty skips webhook registration.
    pub webhook_url: String,
    /// Public base URL of the web app, used for commit-status target links.
    pub web_url: String,
    pub forgejo: ForgejoConfig,
}

pub(crate) struct AppState {
    /// Lazy channels: connect on first use and reconnect on failure, so the
    /// gateway starts cleanly regardless of service start order.
    pub(crate) vcs: VcsClient<Channel>,
    pub(crate) diff: DiffClient<Channel>,
    pub(crate) queue: MergeQueueClient<Channel>,
    pub(crate) compute: ComputeClient<Channel>,
    pub(crate) forgejo: ForgejoClient,
    pub(crate) http: reqwest::Client,
    pub(crate) agent_gateway_url: String,
    pub(crate) agent_admin_token: String,
    pub(crate) webhook_secret: String,
    pub(crate) webhook_url: String,
    pub(crate) web_url: String,
}

fn lazy_channel(url: &str, what: &str) -> Result<Channel, clotho_common::Error> {
    Ok(Channel::from_shared(url.to_string())
        .map_err(|e| clotho_common::Error::Config(format!("{what} url {url:?}: {e}")))?
        .connect_lazy())
}

pub fn router(config: GatewayConfig) -> Result<Router, clotho_common::Error> {
    let state = Arc::new(AppState {
        vcs: VcsClient::new(lazy_channel(&config.vcs_grpc_url, "vcs")?)
            // ExportRepoArchive returns the git object DB; lift the 4 MiB cap.
            .max_decoding_message_size(256 * 1024 * 1024),
        diff: DiffClient::new(lazy_channel(&config.diff_grpc_url, "diff")?),
        queue: MergeQueueClient::new(lazy_channel(&config.merge_queue_grpc_url, "merge queue")?),
        compute: ComputeClient::new(lazy_channel(&config.compute_grpc_url, "compute")?)
            // CI archives can be large; lift the default 4 MiB decode cap.
            .max_decoding_message_size(256 * 1024 * 1024)
            .max_encoding_message_size(256 * 1024 * 1024),
        forgejo: ForgejoClient::new(config.forgejo),
        http: reqwest::Client::new(),
        agent_gateway_url: config.agent_gateway_url.trim_end_matches('/').to_string(),
        agent_admin_token: config.agent_admin_token,
        webhook_secret: config.webhook_secret,
        webhook_url: config.webhook_url,
        web_url: config.web_url.trim_end_matches('/').to_string(),
    });
    Ok(Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/repos", post(create_repo))
        .route("/api/v1/repos", get(repos::list_repos))
        .route("/api/v1/repos/{name}", get(repos::get_repo))
        .route("/api/v1/repos/{name}/tree", get(repos::tree))
        .route("/api/v1/repos/{name}/file", get(repos::file))
        .route(
            "/api/v1/repos/{name}/commits",
            get(repos::commits).post(commit_repo),
        )
        .route("/api/v1/repos/{name}/oplog", get(repos::op_log))
        .route("/api/v1/repos/{name}/submit", post(submit_change))
        .route(
            "/api/v1/repos/{name}/issues",
            get(issues::list_issues).post(issues::create_issue),
        )
        .route(
            "/api/v1/repos/{name}/issues/{number}",
            get(issues::get_issue),
        )
        .route(
            "/api/v1/repos/{name}/issues/{number}/comments",
            post(issues::create_comment),
        )
        .route(
            "/api/v1/repos/{name}/pulls",
            get(pulls::list_pulls).post(pulls::create_pull),
        )
        .route("/api/v1/repos/{name}/pulls/{number}", get(pulls::get_pull))
        .route(
            "/api/v1/repos/{name}/pulls/{number}/comments",
            post(pulls::comment_on_pull),
        )
        .route(
            "/api/v1/repos/{name}/pulls/{number}/reviews",
            post(pulls::review_pull),
        )
        .route(
            "/api/v1/repos/{name}/pulls/{number}/merge",
            post(pulls::merge_pull),
        )
        .route(
            "/api/v1/repos/{name}/pulls/{number}/diff",
            get(pulls::pull_diff),
        )
        .route("/api/v1/repos/{name}/branches", get(status::branches))
        .route(
            "/api/v1/repos/{name}/commits/{sha}/statuses",
            get(status::commit_statuses),
        )
        .route(
            "/api/v1/repos/{name}/agent-sessions",
            get(agents::repo_sessions),
        )
        // Forgejo push webhook → Stage 7 CI (docs/adr/0008). Registered per
        // repo at creation; verified by shared-secret HMAC.
        .route("/api/v1/webhooks/forgejo", post(webhooks::forgejo))
        // The read API is public in the prototype; the web app runs on a
        // different origin in dev (Next on :3100, gateway on :8080).
        .layer(CorsLayer::permissive())
        .with_state(state))
}

async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "clotho-api-gateway",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "ok",
    }))
}

#[derive(Deserialize)]
struct CreateRepoRequest {
    name: String,
}

#[derive(Serialize)]
struct CreateRepoResponse {
    name: String,
    owner: String,
    /// Root operation of the new repo's jj op log.
    operation_id: String,
    /// The empty initial commit that seeds `refs/heads/main`.
    initial_commit_id: String,
    forgejo: RepoInfo,
}

#[derive(Deserialize)]
struct CommitFileRequest {
    path: String,
    content: String,
    #[serde(default)]
    executable: bool,
}

#[derive(Deserialize)]
struct CommitRepoRequest {
    message: String,
    files: Vec<CommitFileRequest>,
    #[serde(default)]
    deleted_paths: Vec<String>,
    #[serde(default)]
    parent_commit_ids: Vec<String>,
    #[serde(default = "default_cli_author_name")]
    author_name: String,
    #[serde(default = "default_cli_author_email")]
    author_email: String,
}

fn default_cli_author_name() -> String {
    "clotho-cli".into()
}

fn default_cli_author_email() -> String {
    "cli@clotho.internal".into()
}

#[derive(Serialize)]
struct CommitRepoResponse {
    commit_id: String,
    change_id: String,
    operation_id: String,
}

#[derive(Deserialize)]
struct SubmitChangeBody {
    commit_id: String,
}

#[derive(Serialize)]
struct SubmitChangeJson {
    commit_id: String,
    change_id: String,
    operation_id: String,
    fast_forwarded: bool,
    conflicted: bool,
    conflicted_paths: Vec<String>,
}

/// One call, two systems: initialize the jj-managed repository in clotho-vcs
/// (which writes the backing bare git repo on the shared volume), then have
/// Forgejo adopt it as a project with issues/PRs. Forgejo never owns the git
/// objects — it reads what the VCS engine writes.
async fn create_repo(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateRepoRequest>,
) -> Result<(StatusCode, Json<CreateRepoResponse>), ApiError> {
    let mut vcs = state.vcs.clone();
    let init = vcs
        .init_repo(InitRepoRequest {
            name: req.name.clone(),
        })
        .await?
        .into_inner();

    // Seed `main` with an empty initial commit before adoption, so Forgejo
    // sees a non-empty repo with a default branch. Forgejo caches emptiness
    // at adoption time; later commits then show up live (docs/adr/0003).
    let initial = vcs
        .commit(CommitRequest {
            repo: req.name.clone(),
            parent_commit_ids: vec![],
            files: vec![],
            deleted_paths: vec![],
            message: "initialize repository".into(),
            author_name: "clotho".into(),
            author_email: "vcs@clotho.internal".into(),
        })
        .await?
        .into_inner();

    let forgejo_repo = state.forgejo.adopt_repo(&req.name).await?;
    tracing::info!(
        repo = %req.name,
        forgejo = %forgejo_repo.full_name,
        "repo provisioned in clotho-vcs and Forgejo"
    );

    // Register the push webhook so future pushes trigger CI (docs/adr/0008).
    // Best-effort: a repo without CI is still a usable repo.
    if !state.webhook_url.is_empty() {
        if let Err(e) = state
            .forgejo
            .create_push_webhook(&req.name, &state.webhook_url, &state.webhook_secret)
            .await
        {
            tracing::warn!(repo = %req.name, error = %e, "failed to register CI push webhook");
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(CreateRepoResponse {
            name: init.name,
            owner: state.forgejo.owner().to_string(),
            operation_id: init.operation_id,
            initial_commit_id: initial.commit_id,
            forgejo: forgejo_repo,
        }),
    ))
}

/// Human/CLI write path: create a commit through the REST edge, while the
/// engine still owns tree construction and git object writes.
async fn commit_repo(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<CommitRepoRequest>,
) -> Result<(StatusCode, Json<CommitRepoResponse>), ApiError> {
    if req.message.trim().is_empty() {
        return Err(ApiError::InvalidRequest("message is required".into()));
    }
    if req.files.is_empty() && req.deleted_paths.is_empty() {
        return Err(ApiError::InvalidRequest(
            "at least one file or deleted path is required".into(),
        ));
    }
    let mut vcs = state.vcs.clone();
    let response = vcs
        .commit(CommitRequest {
            repo: name,
            parent_commit_ids: req.parent_commit_ids,
            files: req
                .files
                .into_iter()
                .map(|f| FileChange {
                    path: f.path,
                    content: f.content.into_bytes(),
                    executable: f.executable,
                })
                .collect(),
            deleted_paths: req.deleted_paths,
            message: req.message,
            author_name: req.author_name,
            author_email: req.author_email,
        })
        .await?
        .into_inner();
    Ok((
        StatusCode::CREATED,
        Json(CommitRepoResponse {
            commit_id: response.commit_id,
            change_id: response.change_id,
            operation_id: response.operation_id,
        }),
    ))
}

/// Submit a commit to the merge queue through the REST edge.
async fn submit_change(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<SubmitChangeBody>,
) -> Result<Json<SubmitChangeJson>, ApiError> {
    let mut queue = state.queue.clone();
    let response = queue
        .submit_change(SubmitChangeRequest {
            repo: name,
            commit_id: req.commit_id,
        })
        .await?
        .into_inner();
    Ok(Json(SubmitChangeJson {
        commit_id: response.commit_id,
        change_id: response.change_id,
        operation_id: response.operation_id,
        fast_forwarded: response.fast_forwarded,
        conflicted: response.conflicted,
        conflicted_paths: response.conflicted_paths,
    }))
}
