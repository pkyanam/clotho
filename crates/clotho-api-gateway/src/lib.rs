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
//!
//! Stage 11 adds Clotho-owned users, orgs, repo ownership, visibility,
//! default-branch metadata, and an activity feed in Postgres. Forgejo is still
//! the internal collaboration provider, but the control plane owns the records.

mod actions;
mod agents;
mod ci;
pub mod control;
pub mod error;
pub mod forgejo;
mod issues;
mod pulls;
mod repos;
mod secrets;
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
use sqlx::postgres::PgPoolOptions;
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
    /// Default compute provider surfaced by the Actions control plane.
    pub compute_provider: String,
    /// Default provider image/snapshot for action jobs.
    pub compute_default_image: String,
    /// Default action timeout.
    pub actions_timeout_seconds: u32,
    /// Env-derived configured flags by provider id (no secrets). Used when
    /// clotho-compute ListProviders is unreachable.
    pub configured_providers: std::collections::HashMap<String, bool>,
    /// Deterministic bootstrap user for the Stage 11 auth placeholder.
    pub bootstrap_user_name: String,
    /// Email for the deterministic bootstrap user.
    pub bootstrap_user_email: String,
    /// Deterministic bootstrap org for the Stage 11 auth placeholder.
    pub bootstrap_org_name: String,
    /// Human-facing name for the bootstrap org.
    pub bootstrap_org_display_name: String,
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
    pub(crate) actions: actions::ActionsState,
    /// Control-plane Postgres pool, shared by Actions and Stage 11 tables.
    pub(crate) pool: Option<sqlx::PgPool>,
    /// Deterministic Stage 11 bootstrap identity.
    pub(crate) bootstrap: control::Bootstrap,
    /// AES-256-GCM master key for secrets at rest (docs/adr/0014). None when
    /// CLOTHO_SECRETS_MASTER_KEY is unset — list still works; write/resolve fail clearly.
    pub(crate) secrets_crypto: Option<secrets::SecretsCrypto>,
}

/// Connect to Postgres and run the embedded gateway migrations.
pub async fn init_db(database_url: &str) -> Result<sqlx::PgPool, clotho_common::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(database_url)
        .await
        .map_err(|e| clotho_common::Error::Config(format!("postgres connect: {e}")))?;
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator
        .run(&pool)
        .await
        .map_err(|e| clotho_common::Error::Config(format!("postgres migrate: {e}")))?;
    Ok(pool)
}

fn lazy_channel(url: &str, what: &str) -> Result<Channel, clotho_common::Error> {
    Ok(Channel::from_shared(url.to_string())
        .map_err(|e| clotho_common::Error::Config(format!("{what} url {url:?}: {e}")))?
        .connect_lazy())
}

pub fn router(config: GatewayConfig) -> Result<Router, clotho_common::Error> {
    let bootstrap = control::Bootstrap::from_config(&config);
    router_with_pool(config, None, bootstrap)
}

pub fn router_with_pool(
    config: GatewayConfig,
    pool: Option<sqlx::PgPool>,
    bootstrap: control::Bootstrap,
) -> Result<Router, clotho_common::Error> {
    let actions_defaults = actions::ActionsDefaults {
        provider: config.compute_provider.to_lowercase(),
        default_image: config.compute_default_image,
        timeout_seconds: config.actions_timeout_seconds,
        configured_providers: config.configured_providers.clone(),
    };
    let actions = match pool {
        Some(ref p) => actions::ActionsState::with_pool(actions_defaults, p.clone()),
        None => actions::ActionsState::new(actions_defaults),
    };
    // Never fail process boot on secrets config: an unset/invalid master key
    // disables write/resolve with a clear log line (docs/adr/0014).
    let secrets_crypto = match secrets::SecretsCrypto::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(
                error = %e,
                "CLOTHO_SECRETS_MASTER_KEY invalid — secret write/resolve disabled"
            );
            None
        }
    };
    if secrets_crypto.is_none() {
        tracing::warn!(
            "CLOTHO_SECRETS_MASTER_KEY unset or invalid — secret write/resolve disabled (docs/adr/0014)"
        );
    }
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
        actions,
        pool,
        bootstrap,
        secrets_crypto,
    });
    Ok(Router::new()
        .route("/healthz", get(healthz))
        // Stage 15: published OpenAPI contract (hand-maintained docs/openapi.yaml).
        .route("/openapi.yaml", get(openapi_yaml))
        // Stage 11: users, orgs, activity, and org-scoped repos.
        .route("/api/v1/users", get(control::list_users_handler))
        .route(
            "/api/v1/orgs",
            get(control::list_orgs_handler).post(control::create_org_handler),
        )
        .route("/api/v1/orgs/{org}", get(control::get_org_handler))
        .route(
            "/api/v1/orgs/{org}/repos",
            get(control::list_org_repos_handler),
        )
        .route("/api/v1/activity", get(control::list_activity_handler))
        // Repo CRUD and browser endpoints (Stage 3/6/11).
        .route("/api/v1/repos", post(create_repo).get(repos::list_repos))
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
            "/api/v1/repos/{name}/actions/runs",
            get(actions::list_runs).post(actions::create_run),
        )
        .route(
            "/api/v1/repos/{name}/actions/runs/{run_id}",
            get(actions::get_run),
        )
        .route(
            "/api/v1/repos/{name}/actions/runs/{run_id}/logs",
            get(actions::get_logs),
        )
        .route(
            "/api/v1/repos/{name}/actions/config",
            get(actions::get_config).put(actions::put_config),
        )
        // Stage 12 canonical provider registry routes (docs/adr/0013).
        .route("/api/v1/providers", get(actions::providers))
        .route("/api/v1/providers/{provider}", get(actions::provider))
        // Stage 10 aliases kept for SDK/web compatibility.
        .route("/api/v1/compute/providers", get(actions::providers))
        .route(
            "/api/v1/compute/providers/{provider}",
            get(actions::provider),
        )
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
        .merge(secrets::routes())
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

/// Hand-maintained OpenAPI 3 document for `/api/v1/*` (Stage 15).
/// Kept in `docs/openapi.yaml` and embedded so the running gateway always
/// serves the same contract checked by `tests/openapi_drift.rs`.
const OPENAPI_YAML: &str = include_str!("../../../docs/openapi.yaml");

async fn openapi_yaml() -> impl axum::response::IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "application/yaml; charset=utf-8",
        )],
        OPENAPI_YAML,
    )
}

#[derive(Serialize)]
struct CreateRepoResponse {
    name: String,
    /// Forgejo/clone-path owner (the git owner, not the Clotho org name).
    owner: String,
    /// Clotho org that owns this repo.
    owner_org: String,
    visibility: String,
    default_branch: String,
    clone_url: String,
    provider: String,
    configured: bool,
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
/// objects — it reads what the VCS engine writes. Stage 11 persists Clotho
/// repo ownership/visibility/default-branch metadata to Postgres first.
async fn create_repo(
    State(state): State<Arc<AppState>>,
    Json(req): Json<control::CreateRepoRequest>,
) -> Result<(StatusCode, Json<CreateRepoResponse>), ApiError> {
    control::valid_name(&req.name)?;

    // Resolve the owning org before provisioning anything.
    let owner_org = if req.owner_org.is_empty() {
        state.bootstrap.org_name.clone()
    } else {
        req.owner_org.clone()
    };
    let (org_id, org_name, forgejo_owner) = match &state.pool {
        Some(pool) => control::resolve_org(pool, &state.bootstrap, &owner_org).await?,
        None => (
            state.bootstrap.org_id.clone(),
            state.bootstrap.org_name.clone(),
            state.forgejo.owner().to_string(),
        ),
    };

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

    // Persist the Clotho control-plane record so the web app owns repo state.
    let clotho_repo = if let Some(pool) = &state.pool {
        Some(
            control::insert_repo(
                pool,
                &state.bootstrap,
                &req,
                &(org_id, org_name.clone(), forgejo_owner.clone()),
                &forgejo_repo,
            )
            .await?,
        )
    } else {
        None
    };

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

    let provider = state.actions.default_provider();
    let configured = state.actions.provider_configured(&provider);
    let base_url = state.forgejo.config().base_url.clone();
    let mut repo_info = match &clotho_repo {
        Some(c) => {
            control::build_repo_info(c, Some(&forgejo_repo), &base_url, &provider, configured)
        }
        None => {
            control::fallback_repo_info(&req.name, &forgejo_owner, &base_url, &provider, configured)
        }
    };
    repo_info.owner = forgejo_owner.clone();
    repo_info.clone_url = format!(
        "{base}/{forgejo_owner}/{name}.git",
        base = base_url.trim_end_matches('/'),
        name = req.name
    );

    Ok((
        StatusCode::CREATED,
        Json(CreateRepoResponse {
            name: init.name,
            owner: forgejo_owner,
            owner_org: org_name,
            visibility: req.visibility,
            default_branch: req.default_branch,
            clone_url: repo_info.clone_url.clone(),
            provider,
            configured,
            operation_id: init.operation_id,
            initial_commit_id: initial.commit_id,
            forgejo: repo_info,
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
