//! Operator-facing admin surface: create agents, mint scoped tokens, read
//! the audit log. Guarded by a single admin bearer token from the service
//! environment — deliberately boring for the prototype; humans get real
//! OAuth in the collaboration layer, not here.

use std::sync::Arc;

use axum::extract::{Path, Query, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::identity::{sha256, AuthorizationDecision, IdentityError, IdentityStore};

pub struct AdminState {
    pub identity: IdentityStore,
    /// SHA-256 of the admin bearer token; compared in hashed form.
    pub admin_token_hash: Vec<u8>,
}

pub fn router(state: Arc<AdminState>) -> Router {
    Router::new()
        .route("/admin/v1/agents", get(list_agents).post(create_agent))
        .route("/admin/v1/agents/{name}", get(get_agent))
        .route(
            "/admin/v1/agents/{name}/tokens",
            get(list_tokens).post(mint_token),
        )
        .route(
            "/admin/v1/agents/{name}/tokens/{token_id}",
            delete(revoke_token).patch(update_token_scopes),
        )
        .route("/admin/v1/agents/{name}/audit", get(audit_log))
        .route("/admin/v1/repos/{repo}/sessions", get(repo_sessions))
        .route("/admin/v1/authorize", post(authorize_agent))
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
        IdentityError::AgentNotFound(_) | IdentityError::TokenNotFound(_, _) => {
            StatusCode::NOT_FOUND
        }
        IdentityError::AgentExists(_) => StatusCode::CONFLICT,
        IdentityError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error(status, err)
}

#[derive(serde::Deserialize)]
struct AuthorizeAgentRequest {
    token: String,
    /// Exact repository selected by the API handler. Empty denotes a
    /// platform tool whose current MCP contract has no repository scope.
    repo: String,
    /// Exact MCP tool selected by the API handler, never by an external
    /// delegation header.
    tool: String,
}

#[derive(serde::Serialize)]
struct AuthorizedAgentResponse {
    agent_id: Uuid,
    token_id: Uuid,
    agent: String,
    /// Deliberate internal-only disclosure: the API needs repository scope to
    /// filter list/activity rows before pagination. The admin-authenticated
    /// caller learns no bearer or allowed-tool set, and this response is
    /// returned only after the exact requested tool has been authorized.
    allowed_repos: Vec<String>,
}

/// Internal scope introspection for the API gateway. The presented bearer is
/// consumed only by the agent identity owner and never appears in a response,
/// trace field, or error message.
async fn authorize_agent(
    State(state): State<Arc<AdminState>>,
    Json(req): Json<AuthorizeAgentRequest>,
) -> Response {
    if req.tool.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "tool is required");
    }
    match state
        .identity
        .authorize(&req.token, &req.repo, &req.tool)
        .await
    {
        Ok(AuthorizationDecision::Authorized(agent)) => Json(AuthorizedAgentResponse {
            agent_id: agent.agent_id,
            token_id: agent.token_id,
            agent: agent.name,
            allowed_repos: agent.allowed_repos,
        })
        .into_response(),
        Ok(AuthorizationDecision::InvalidCredential) => error(
            StatusCode::UNAUTHORIZED,
            "agent credential is invalid or inactive",
        ),
        Ok(AuthorizationDecision::ScopeDenied) => error(
            StatusCode::FORBIDDEN,
            "agent credential does not authorize the requested scope",
        ),
        Err(err) => {
            tracing::error!(error = %err, "agent scope introspection failed");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "agent authorization is unavailable",
            )
        }
    }
}

#[derive(serde::Deserialize)]
struct ListAgentsQuery {
    #[serde(default)]
    include_disabled: bool,
}

async fn list_agents(
    State(state): State<Arc<AdminState>>,
    Query(query): Query<ListAgentsQuery>,
) -> Response {
    match state.identity.list_agents(query.include_disabled).await {
        Ok(agents) => Json(json!({ "agents": agents })).into_response(),
        Err(err) => identity_error(err),
    }
}

async fn get_agent(State(state): State<Arc<AdminState>>, Path(name): Path<String>) -> Response {
    match state.identity.get_agent(&name).await {
        Ok(agent) => match state.identity.list_tokens(&name).await {
            Ok(tokens) => Json(json!({ "agent": agent, "tokens": tokens })).into_response(),
            Err(err) => identity_error(err),
        },
        Err(err) => identity_error(err),
    }
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

async fn list_tokens(State(state): State<Arc<AdminState>>, Path(name): Path<String>) -> Response {
    match state.identity.list_tokens(&name).await {
        Ok(tokens) => Json(json!({ "tokens": tokens })).into_response(),
        Err(err) => identity_error(err),
    }
}

async fn revoke_token(
    State(state): State<Arc<AdminState>>,
    Path((name, token_id)): Path<(String, Uuid)>,
) -> Response {
    match state.identity.revoke_token(&name, token_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => identity_error(err),
    }
}

#[derive(serde::Deserialize)]
struct UpdateTokenScopesRequest {
    allowed_repos: Option<Vec<String>>,
    allowed_tools: Option<Vec<String>>,
}

async fn update_token_scopes(
    State(state): State<Arc<AdminState>>,
    Path((name, token_id)): Path<(String, Uuid)>,
    Json(req): Json<UpdateTokenScopesRequest>,
) -> Response {
    if req.allowed_repos.is_none() && req.allowed_tools.is_none() {
        return error(
            StatusCode::BAD_REQUEST,
            "at least one of allowed_repos or allowed_tools is required",
        );
    }
    match state
        .identity
        .update_token_scopes(&name, token_id, req.allowed_repos, req.allowed_tools)
        .await
    {
        Ok(token) => Json(token).into_response(),
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

#[derive(serde::Deserialize)]
struct SessionsQuery {
    #[serde(default = "default_sessions_limit")]
    limit: i64,
    /// Look-back window in seconds (default: 7 days).
    #[serde(default = "default_sessions_within_secs")]
    within_secs: i64,
}

fn default_sessions_limit() -> i64 {
    20
}

fn default_sessions_within_secs() -> i64 {
    7 * 24 * 3600
}

/// Recent agent sessions on one repo, aggregated from the audit log — the
/// presence data the Stage 6 UI polls (via the api-gateway proxy, ADR-0007).
async fn repo_sessions(
    State(state): State<Arc<AdminState>>,
    Path(repo): Path<String>,
    Query(query): Query<SessionsQuery>,
) -> Response {
    match state
        .identity
        .repo_sessions(
            &repo,
            query.within_secs.clamp(1, 365 * 24 * 3600),
            query.limit.clamp(1, 1000),
        )
        .await
    {
        Ok(sessions) => Json(sessions).into_response(),
        Err(err) => identity_error(err),
    }
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;

    use futures::FutureExt as _;

    use super::*;

    fn test_database_url() -> String {
        std::env::var("CLOTHO_AGENT_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("CLOTHO_AGENT_DATABASE_URL"))
            .unwrap_or_else(|_| "postgres://clotho:clotho-dev@localhost:5432/clotho".into())
    }

    async fn post_authorize(
        base_url: &str,
        admin_token: &str,
        agent_token: &str,
        repo: &str,
        tool: &str,
    ) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{base_url}/admin/v1/authorize"))
            .bearer_auth(admin_token)
            .json(&json!({
                "token": agent_token,
                "repo": repo,
                "tool": tool,
            }))
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn introspection_revalidates_identity_expiry_revocation_and_both_scopes() {
        let Ok(pool) = crate::init_db(&test_database_url()).await else {
            eprintln!("skipping agent scope-introspection test: Postgres unavailable");
            return;
        };
        let identity = IdentityStore::new(pool.clone());
        let suffix = Uuid::new_v4().simple().to_string();
        let agent_name = format!("authorize-{suffix}");
        let admin_token = "agent-scope-introspection-admin-test";
        let agent = identity.create_agent(&agent_name, "test").await.unwrap();
        let valid = identity
            .mint_token(
                &agent_name,
                vec!["repo-a".into()],
                vec!["get_file".into()],
                None,
            )
            .await
            .unwrap();
        let expired = identity
            .mint_token(
                &agent_name,
                vec!["repo-a".into()],
                vec!["get_file".into()],
                Some(Utc::now() - Duration::seconds(1)),
            )
            .await
            .unwrap();
        let wildcard = identity
            .mint_token(
                &agent_name,
                vec!["*".into()],
                vec!["get_activity".into()],
                None,
            )
            .await
            .unwrap();

        let app = router(Arc::new(AdminState {
            identity: identity.clone(),
            admin_token_hash: sha256(admin_token.as_bytes()),
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let base_url = format!("http://{address}");

        let outcome = AssertUnwindSafe(async {
            let accepted =
                post_authorize(&base_url, admin_token, &valid.token, "repo-a", "get_file").await;
            assert_eq!(accepted.status(), StatusCode::OK);
            let accepted_body = accepted.text().await.unwrap();
            assert!(!accepted_body.contains(&valid.token));
            assert!(!accepted_body.contains("allowed_tools"));
            let accepted_json: serde_json::Value = serde_json::from_str(&accepted_body).unwrap();
            assert_eq!(accepted_json["agent_id"], agent.id.to_string());
            assert_eq!(accepted_json["token_id"], valid.token_id.to_string());
            assert_eq!(accepted_json["agent"], agent_name);
            assert_eq!(accepted_json["allowed_repos"], json!(["repo-a"]));

            let wildcard_response =
                post_authorize(&base_url, admin_token, &wildcard.token, "", "get_activity").await;
            assert_eq!(wildcard_response.status(), StatusCode::OK);
            let wildcard_body = wildcard_response.text().await.unwrap();
            assert!(!wildcard_body.contains(&wildcard.token));
            assert!(!wildcard_body.contains("allowed_tools"));
            let wildcard_json: serde_json::Value = serde_json::from_str(&wildcard_body).unwrap();
            assert_eq!(wildcard_json["allowed_repos"], json!(["*"]));

            for (repo, tool) in [("repo-b", "get_file"), ("repo-a", "get_tree")] {
                let denied = post_authorize(&base_url, admin_token, &valid.token, repo, tool).await;
                assert_eq!(denied.status(), StatusCode::FORBIDDEN);
                let body = denied.text().await.unwrap();
                assert!(!body.contains(&valid.token));
                assert!(!body.contains(repo));
                assert!(!body.contains(tool));
            }

            let expired_response =
                post_authorize(&base_url, admin_token, &expired.token, "repo-a", "get_file").await;
            assert_eq!(expired_response.status(), StatusCode::UNAUTHORIZED);
            assert!(!expired_response
                .text()
                .await
                .unwrap()
                .contains(&expired.token));

            identity
                .revoke_token(&agent_name, valid.token_id)
                .await
                .unwrap();
            let revoked =
                post_authorize(&base_url, admin_token, &valid.token, "repo-a", "get_file").await;
            assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
            assert!(!revoked.text().await.unwrap().contains(&valid.token));

            let bad_admin = post_authorize(
                &base_url,
                "wrong-admin-token",
                &expired.token,
                "repo-a",
                "get_file",
            )
            .await;
            assert_eq!(bad_admin.status(), StatusCode::UNAUTHORIZED);
        })
        .catch_unwind()
        .await;

        server.abort();
        let _ = sqlx::query("delete from agents where id = $1")
            .bind(agent.id)
            .execute(&pool)
            .await;
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    }
}
