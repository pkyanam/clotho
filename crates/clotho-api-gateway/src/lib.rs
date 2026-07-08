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
pub mod error;
pub mod forgejo;
mod pulls;
mod repos;

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use clotho_common::pb::diff::v1::diff_client::DiffClient;
use clotho_common::pb::vcs::v1::vcs_client::VcsClient;
use clotho_common::pb::vcs::v1::{CommitRequest, InitRepoRequest};
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
    /// clotho-agent-gateway admin HTTP base, e.g. `http://clotho-agent-gateway:8090`.
    pub agent_gateway_url: String,
    /// Admin bearer token for the agent gateway (service-to-service).
    pub agent_admin_token: String,
    pub forgejo: ForgejoConfig,
}

pub(crate) struct AppState {
    /// Lazy channels: connect on first use and reconnect on failure, so the
    /// gateway starts cleanly regardless of service start order.
    pub(crate) vcs: VcsClient<Channel>,
    pub(crate) diff: DiffClient<Channel>,
    pub(crate) forgejo: ForgejoClient,
    pub(crate) http: reqwest::Client,
    pub(crate) agent_gateway_url: String,
    pub(crate) agent_admin_token: String,
}

fn lazy_channel(url: &str, what: &str) -> Result<Channel, clotho_common::Error> {
    Ok(Channel::from_shared(url.to_string())
        .map_err(|e| clotho_common::Error::Config(format!("{what} url {url:?}: {e}")))?
        .connect_lazy())
}

pub fn router(config: GatewayConfig) -> Result<Router, clotho_common::Error> {
    let state = Arc::new(AppState {
        vcs: VcsClient::new(lazy_channel(&config.vcs_grpc_url, "vcs")?),
        diff: DiffClient::new(lazy_channel(&config.diff_grpc_url, "diff")?),
        forgejo: ForgejoClient::new(config.forgejo),
        http: reqwest::Client::new(),
        agent_gateway_url: config.agent_gateway_url.trim_end_matches('/').to_string(),
        agent_admin_token: config.agent_admin_token,
    });
    Ok(Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/repos", post(create_repo))
        .route("/api/v1/repos", get(repos::list_repos))
        .route("/api/v1/repos/{name}", get(repos::get_repo))
        .route("/api/v1/repos/{name}/tree", get(repos::tree))
        .route("/api/v1/repos/{name}/file", get(repos::file))
        .route("/api/v1/repos/{name}/commits", get(repos::commits))
        .route("/api/v1/repos/{name}/oplog", get(repos::op_log))
        .route("/api/v1/repos/{name}/pulls", get(pulls::list_pulls))
        .route("/api/v1/repos/{name}/pulls/{number}", get(pulls::get_pull))
        .route(
            "/api/v1/repos/{name}/pulls/{number}/diff",
            get(pulls::pull_diff),
        )
        .route(
            "/api/v1/repos/{name}/agent-sessions",
            get(agents::repo_sessions),
        )
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
