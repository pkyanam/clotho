//! Edge API gateway — Clotho's REST/JSON boundary (docs/prd.md §2).
//!
//! Aggregates the internal gRPC services and Forgejo's REST API behind one
//! boring, debuggable HTTP surface. Stage 3 lands the first real endpoint:
//! `POST /api/v1/repos` provisions a clotho-vcs repository (jj-native, real
//! git objects) *and* its Forgejo project in a single call (docs/adr/0003).

pub mod error;
pub mod forgejo;

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use clotho_common::pb::vcs::v1::vcs_client::VcsClient;
use clotho_common::pb::vcs::v1::{CommitRequest, InitRepoRequest};
use serde::{Deserialize, Serialize};
use tonic::transport::Channel;

use crate::error::ApiError;
use crate::forgejo::{ForgejoClient, ForgejoConfig, RepoInfo};

#[derive(Clone)]
pub struct GatewayConfig {
    /// clotho-vcs gRPC endpoint, e.g. `http://clotho-vcs:50051`.
    pub vcs_grpc_url: String,
    pub forgejo: ForgejoConfig,
}

struct AppState {
    /// Lazy channel: connects on first use and reconnects on failure, so the
    /// gateway starts cleanly regardless of service start order.
    vcs: VcsClient<Channel>,
    forgejo: ForgejoClient,
}

pub fn router(config: GatewayConfig) -> Result<Router, clotho_common::Error> {
    let channel = Channel::from_shared(config.vcs_grpc_url.clone())
        .map_err(|e| {
            clotho_common::Error::Config(format!("vcs url {:?}: {e}", config.vcs_grpc_url))
        })?
        .connect_lazy();
    let state = Arc::new(AppState {
        vcs: VcsClient::new(channel),
        forgejo: ForgejoClient::new(config.forgejo),
    });
    Ok(Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/repos", post(create_repo))
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
