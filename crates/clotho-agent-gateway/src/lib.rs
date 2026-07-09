//! Agent interface layer (docs/prd.md §5 Stage 4): a native MCP server over
//! streamable HTTP exposing read/checkpoint tools plus write tools
//! (`commit`, `submit_change`), backed by Clotho services over gRPC.
//!
//! Agents authenticate with scoped bearer tokens (Postgres-backed, a model
//! fully distinct from human identities — see the `identity` module and
//! ADR-0005); every tool invocation is audited. A small admin REST surface
//! creates agents and mints tokens.

pub mod admin;
pub mod identity;
pub mod mcp;
pub mod rest;

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::{
    StreamableHttpServerConfig, StreamableHttpService,
};
use sqlx::postgres::PgPoolOptions;
use tonic::transport::Channel;

use crate::admin::AdminState;
use crate::identity::{sha256, IdentityStore};
use crate::mcp::AgentGateway;
use crate::rest::RestClient;

/// Path the MCP endpoint is served on.
pub const MCP_PATH: &str = "/mcp";

#[derive(Clone)]
pub struct GatewayConfig {
    /// Control-plane Postgres, e.g. `postgres://clotho:...@postgres:5432/clotho`.
    pub database_url: String,
    /// clotho-vcs gRPC endpoint, e.g. `http://clotho-vcs:50051`.
    pub vcs_grpc_url: String,
    /// clotho-diff gRPC endpoint, e.g. `http://clotho-diff:50055`.
    pub diff_grpc_url: String,
    /// clotho-merge-queue gRPC endpoint, e.g. `http://clotho-merge-queue:50053`.
    pub merge_queue_grpc_url: String,
    /// Clotho API gateway REST base URL for Stage 15 collab/Actions tools.
    /// e.g. `http://clotho-api-gateway:8080` in compose, `http://localhost:8080` locally.
    pub api_url: String,
    /// Bearer token guarding the admin surface (create agents, mint tokens).
    pub admin_token: String,
}

/// Connect to Postgres and run the embedded migrations.
pub async fn init_db(database_url: &str) -> Result<sqlx::PgPool, clotho_common::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(database_url)
        .await
        .map_err(|e| clotho_common::Error::Config(format!("postgres connect: {e}")))?;
    // Share the `clotho` DB with clotho-api-gateway (Actions + control-plane
    // migrations 1001/1002). Ignore versions we don't own so the shared
    // `_sqlx_migrations` table does not break startup.
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator
        .run(&pool)
        .await
        .map_err(|e| clotho_common::Error::Config(format!("postgres migrate: {e}")))?;
    Ok(pool)
}

/// Assemble the full HTTP surface: the MCP endpoint (auth-gated), the admin
/// endpoints, and a plain healthz.
pub fn router(config: &GatewayConfig, pool: sqlx::PgPool) -> Result<Router, clotho_common::Error> {
    let lazy_channel = |url: &str| -> Result<Channel, clotho_common::Error> {
        // Lazy channels: connect on first use and reconnect on failure, so
        // the gateway starts cleanly regardless of service start order.
        Ok(Channel::from_shared(url.to_string())
            .map_err(|e| clotho_common::Error::Config(format!("grpc url {url:?}: {e}")))?
            .connect_lazy())
    };
    let identity = IdentityStore::new(pool);
    let rest = RestClient::new(&config.api_url);
    let gateway = AgentGateway::new(
        lazy_channel(&config.vcs_grpc_url)?,
        lazy_channel(&config.diff_grpc_url)?,
        lazy_channel(&config.merge_queue_grpc_url)?,
        rest,
        identity.clone(),
    );

    let mcp_service = StreamableHttpService::new(
        move || Ok(gateway.clone()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );
    let mcp = Router::new().nest_service(MCP_PATH, mcp_service).layer(
        axum::middleware::from_fn_with_state(identity.clone(), authenticate_agent),
    );

    let admin = admin::router(Arc::new(AdminState {
        identity,
        admin_token_hash: sha256(config.admin_token.as_bytes()),
    }));

    Ok(Router::new()
        .route("/healthz", axum::routing::get(healthz))
        .merge(mcp)
        .merge(admin))
}

async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "clotho-agent-gateway",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "ok",
    }))
}

/// MCP auth middleware: resolve the bearer token to an agent identity and
/// inject it into the request extensions, where the streamable HTTP
/// transport forwards it (via `http::request::Parts`) to every tool handler.
async fn authenticate_agent(
    State(identity): State<IdentityStore>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(token) = admin::bearer_token(&request).map(str::to_owned) else {
        return unauthorized("missing bearer token");
    };
    match identity.authenticate(&token).await {
        Ok(Some(agent)) => {
            request.extensions_mut().insert(agent);
            next.run(request).await
        }
        Ok(None) => unauthorized("unknown, revoked, or expired agent token"),
        Err(err) => {
            tracing::error!(error = %err, "token lookup failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "token lookup failed" })),
            )
                .into_response()
        }
    }
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}
