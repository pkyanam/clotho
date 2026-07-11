//! Agent admin and presence: proxies the agent gateway's identity store
//! through the public REST edge (Slice C, ADR-0016). Humans authenticate
//! with Clotho API tokens; the edge calls agent-gateway with
//! `CLOTHO_AGENT_ADMIN_TOKEN` service-to-service.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use reqwest::Method;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::{self, AuthContext};
use crate::error::ApiError;
use crate::AppState;

#[derive(Deserialize)]
pub struct SessionsQuery {
    #[serde(default = "default_sessions_limit")]
    pub limit: i64,
    /// Look-back window in seconds (default: 7 days).
    #[serde(default = "default_sessions_within_secs")]
    pub within_secs: i64,
}

fn default_sessions_limit() -> i64 {
    20
}

fn default_sessions_within_secs() -> i64 {
    7 * 24 * 3600
}

#[derive(Deserialize)]
pub(crate) struct AuditQuery {
    #[serde(default = "default_audit_limit")]
    limit: i64,
}

fn default_audit_limit() -> i64 {
    50
}

#[derive(Deserialize)]
pub(crate) struct CreateAgentBody {
    name: String,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
pub(crate) struct MintTokenBody {
    allowed_repos: Vec<String>,
    allowed_tools: Vec<String>,
    expires_in_secs: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct UpdateTokenScopesBody {
    allowed_repos: Option<Vec<String>>,
    allowed_tools: Option<Vec<String>>,
}

fn ensure_agent_admin_configured(state: &AppState) -> Result<(), ApiError> {
    if state.agent_admin_token.trim().is_empty() {
        return Err(ApiError::Internal(
            "agent management is not configured".into(),
        ));
    }
    Ok(())
}

/// Bootstrap user or any org admin may manage agent identities (ADR-0016).
async fn require_agent_admin(state: &AppState, auth: &AuthContext) -> Result<(), ApiError> {
    ensure_agent_admin_configured(state)?;
    if auth.user_id == state.bootstrap.user_id {
        return Ok(());
    }
    let Some(pool) = state.pool.as_ref() else {
        if !state.auth_required {
            return Ok(());
        }
        return Err(ApiError::Forbidden(
            "requires bootstrap user or org admin".into(),
        ));
    };
    let count: i64 = sqlx::query_scalar(
        "select count(*)::bigint from org_memberships where user_id = $1 and role = 'admin'",
    )
    .bind(&auth.user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("org admin lookup: {e}")))?;
    if count > 0 {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "requires bootstrap user or org admin".into(),
    ))
}

async fn proxy_agent_admin(
    state: &AppState,
    method: Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<reqwest::Response, ApiError> {
    let url = format!("{}{path}", state.agent_gateway_url);
    let mut req = state
        .http
        .request(method, &url)
        .bearer_auth(&state.agent_admin_token);
    if let Some(body) = body {
        req = req.json(&body);
    }
    req.send()
        .await
        .map_err(|e| ApiError::Upstream(format!("agent-gateway: {e}")))
}

async fn map_upstream(
    response: reqwest::Response,
    context: &str,
) -> Result<axum::response::Response, ApiError> {
    let status = response.status();
    if status.is_success() || status == StatusCode::NO_CONTENT {
        if status == StatusCode::NO_CONTENT {
            return Ok(StatusCode::NO_CONTENT.into_response());
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ApiError::Upstream(format!("{context}: invalid json: {e}")))?;
        return Ok(Json(body).into_response());
    }
    let body = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or(body);
    Err(match status {
        StatusCode::NOT_FOUND => ApiError::NotFound(message),
        StatusCode::CONFLICT => ApiError::Conflict(message),
        StatusCode::BAD_REQUEST => ApiError::InvalidRequest(message),
        StatusCode::UNAUTHORIZED => ApiError::Unauthorized(message),
        StatusCode::FORBIDDEN => ApiError::Forbidden(message),
        _ => ApiError::Upstream(format!("{context}: {status}: {message}")),
    })
}

pub async fn list_agents(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    require_agent_admin(&state, &auth).await?;
    let response = proxy_agent_admin(&state, Method::GET, "/admin/v1/agents", None).await?;
    map_upstream(response, "agent-gateway: list agents").await
}

pub async fn create_agent(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateAgentBody>,
) -> Result<impl IntoResponse, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    require_agent_admin(&state, &auth).await?;
    let response = proxy_agent_admin(
        &state,
        Method::POST,
        "/admin/v1/agents",
        Some(serde_json::json!({
            "name": body.name,
            "description": body.description,
        })),
    )
    .await?;
    let status = response.status();
    map_upstream_with_status(response, status, "agent-gateway: create agent").await
}

pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    require_agent_admin(&state, &auth).await?;
    let response = proxy_agent_admin(
        &state,
        Method::GET,
        &format!("/admin/v1/agents/{name}"),
        None,
    )
    .await?;
    map_upstream(response, "agent-gateway: get agent").await
}

pub async fn mint_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<MintTokenBody>,
) -> Result<impl IntoResponse, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    require_agent_admin(&state, &auth).await?;
    let response = proxy_agent_admin(
        &state,
        Method::POST,
        &format!("/admin/v1/agents/{name}/tokens"),
        Some(serde_json::json!({
            "allowed_repos": body.allowed_repos,
            "allowed_tools": body.allowed_tools,
            "expires_in_secs": body.expires_in_secs,
        })),
    )
    .await?;
    let status = response.status();
    map_upstream_with_status(response, status, "agent-gateway: mint token").await
}

pub async fn list_tokens(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    require_agent_admin(&state, &auth).await?;
    let response = proxy_agent_admin(
        &state,
        Method::GET,
        &format!("/admin/v1/agents/{name}/tokens"),
        None,
    )
    .await?;
    map_upstream(response, "agent-gateway: list tokens").await
}

pub async fn revoke_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, token_id)): Path<(String, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    require_agent_admin(&state, &auth).await?;
    let response = proxy_agent_admin(
        &state,
        Method::DELETE,
        &format!("/admin/v1/agents/{name}/tokens/{token_id}"),
        None,
    )
    .await?;
    map_upstream(response, "agent-gateway: revoke token").await
}

pub async fn update_token_scopes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, token_id)): Path<(String, Uuid)>,
    Json(body): Json<UpdateTokenScopesBody>,
) -> Result<impl IntoResponse, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    require_agent_admin(&state, &auth).await?;
    let response = proxy_agent_admin(
        &state,
        Method::PATCH,
        &format!("/admin/v1/agents/{name}/tokens/{token_id}"),
        Some(serde_json::json!({
            "allowed_repos": body.allowed_repos,
            "allowed_tools": body.allowed_tools,
        })),
    )
    .await?;
    map_upstream(response, "agent-gateway: update token scopes").await
}

pub async fn agent_audit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    require_agent_admin(&state, &auth).await?;
    let url = format!(
        "{}/admin/v1/agents/{name}/audit?limit={}",
        state.agent_gateway_url,
        query.limit.clamp(1, 1000)
    );
    let response = state
        .http
        .get(&url)
        .bearer_auth(&state.agent_admin_token)
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("agent-gateway: {e}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError::Upstream(format!(
            "agent-gateway: audit returned {status}: {body}"
        )));
    }
    let entries: serde_json::Value = response
        .json()
        .await
        .map_err(|e| ApiError::Upstream(format!("agent-gateway: invalid audit body: {e}")))?;
    Ok(Json(serde_json::json!({ "entries": entries })))
}

pub async fn repo_sessions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(query): Query<SessionsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    auth::require_repo_read(&headers, &state, &name).await?;
    ensure_agent_admin_configured(&state)?;
    let url = format!(
        "{}/admin/v1/repos/{name}/sessions?limit={}&within_secs={}",
        state.agent_gateway_url, query.limit, query.within_secs
    );
    let response = state
        .http
        .get(&url)
        .bearer_auth(&state.agent_admin_token)
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("agent-gateway: {e}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError::Upstream(format!(
            "agent-gateway: sessions returned {status}: {body}"
        )));
    }
    let sessions: serde_json::Value = response
        .json()
        .await
        .map_err(|e| ApiError::Upstream(format!("agent-gateway: invalid sessions body: {e}")))?;
    Ok(Json(serde_json::json!({ "sessions": sessions })))
}

async fn map_upstream_with_status(
    response: reqwest::Response,
    status: StatusCode,
    context: &str,
) -> Result<axum::response::Response, ApiError> {
    if !status.is_success() {
        return map_upstream(response, context).await;
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| ApiError::Upstream(format!("{context}: invalid json: {e}")))?;
    Ok((status, Json(body)).into_response())
}
