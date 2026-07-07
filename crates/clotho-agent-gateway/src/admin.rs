//! Operator-facing admin surface: create agents, mint scoped tokens, read
//! the audit log. Guarded by a single admin bearer token from the service
//! environment — deliberately boring for the prototype; humans get real
//! OAuth in the collaboration layer, not here.

use std::sync::Arc;

use axum::extract::{Path, Query, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use serde_json::json;

use crate::identity::{sha256, IdentityError, IdentityStore};

pub struct AdminState {
    pub identity: IdentityStore,
    /// SHA-256 of the admin bearer token; compared in hashed form.
    pub admin_token_hash: Vec<u8>,
}

pub fn router(state: Arc<AdminState>) -> Router {
    Router::new()
        .route("/admin/v1/agents", post(create_agent))
        .route("/admin/v1/agents/{name}/tokens", post(mint_token))
        .route("/admin/v1/agents/{name}/audit", get(audit_log))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_admin,
        ))
        .with_state(state)
}

async fn require_admin(
    State(state): State<Arc<AdminState>>,
    request: Request,
    next: Next,
) -> Response {
    let authorized = bearer_token(&request)
        .is_some_and(|token| sha256(token.as_bytes()) == state.admin_token_hash);
    if !authorized {
        return error(StatusCode::UNAUTHORIZED, "admin token required");
    }
    next.run(request).await
}

pub fn bearer_token(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

fn error(status: StatusCode, message: impl std::fmt::Display) -> Response {
    (status, Json(json!({ "error": message.to_string() }))).into_response()
}

fn identity_error(err: IdentityError) -> Response {
    let status = match &err {
        IdentityError::AgentNotFound(_) => StatusCode::NOT_FOUND,
        IdentityError::AgentExists(_) => StatusCode::CONFLICT,
        IdentityError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error(status, err)
}

#[derive(serde::Deserialize)]
struct CreateAgentRequest {
    name: String,
    #[serde(default)]
    description: String,
}

async fn create_agent(
    State(state): State<Arc<AdminState>>,
    Json(req): Json<CreateAgentRequest>,
) -> Response {
    match state
        .identity
        .create_agent(&req.name, &req.description)
        .await
    {
        Ok(agent) => {
            tracing::info!(agent = %agent.name, "agent identity created");
            (StatusCode::CREATED, Json(agent)).into_response()
        }
        Err(err) => identity_error(err),
    }
}

#[derive(serde::Deserialize)]
struct MintTokenRequest {
    /// Repos this token may touch; `"*"` grants all.
    allowed_repos: Vec<String>,
    /// MCP tools this token may call; `"*"` grants all.
    allowed_tools: Vec<String>,
    /// Optional lifetime; omitted means the token does not expire.
    expires_in_secs: Option<i64>,
}

async fn mint_token(
    State(state): State<Arc<AdminState>>,
    Path(name): Path<String>,
    Json(req): Json<MintTokenRequest>,
) -> Response {
    let expires_at = req
        .expires_in_secs
        .map(|s| Utc::now() + Duration::seconds(s));
    match state
        .identity
        .mint_token(&name, req.allowed_repos, req.allowed_tools, expires_at)
        .await
    {
        Ok(minted) => {
            tracing::info!(agent = %name, token_id = %minted.token_id, "agent token minted");
            (StatusCode::CREATED, Json(minted)).into_response()
        }
        Err(err) => identity_error(err),
    }
}

#[derive(serde::Deserialize)]
struct AuditQuery {
    #[serde(default = "default_audit_limit")]
    limit: i64,
}

fn default_audit_limit() -> i64 {
    50
}

async fn audit_log(
    State(state): State<Arc<AdminState>>,
    Path(name): Path<String>,
    Query(query): Query<AuditQuery>,
) -> Response {
    match state
        .identity
        .audit_log(&name, query.limit.clamp(1, 1000))
        .await
    {
        Ok(entries) => Json(entries).into_response(),
        Err(err) => identity_error(err),
    }
}
