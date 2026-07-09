//! Edge API gateway — REST aggregation over Clotho services and Forgejo.
//!
//! Serves REST/JSON on `CLOTHO_GATEWAY_HTTP_ADDR` (default 0.0.0.0:8080) and
//! the standard Clotho gRPC health check on the usual service port.

use std::net::SocketAddr;

use clotho_api_gateway::control::{self, Bootstrap};
use clotho_api_gateway::forgejo::{ForgejoConfig, TokenSource};
use clotho_api_gateway::GatewayConfig;
use clotho_common::{health, telemetry, Error};
use tonic::transport::Server;

const SERVICE: &str = "clotho-api-gateway";
const DEFAULT_PORT: u16 = 50056;

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn env_u32_or(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    telemetry::init();
    let grpc_addr = health::addr_from_env(DEFAULT_PORT)?;
    let http_addr: SocketAddr = env_or("CLOTHO_GATEWAY_HTTP_ADDR", "0.0.0.0:8080")
        .parse()
        .map_err(|e| Error::Config(format!("CLOTHO_GATEWAY_HTTP_ADDR: {e}")))?;

    // Prefer an inline token (CI/tests); the dev stack provisions a token
    // file on a shared volume (scripts/forgejo/provision.sh).
    let token = match std::env::var("CLOTHO_FORGEJO_TOKEN") {
        Ok(token) => TokenSource::Inline(token),
        Err(_) => TokenSource::File(
            env_or("CLOTHO_FORGEJO_TOKEN_FILE", "/run/clotho/forgejo-token").into(),
        ),
    };
    let config = GatewayConfig {
        vcs_grpc_url: env_or("CLOTHO_VCS_GRPC_URL", "http://localhost:50051"),
        diff_grpc_url: env_or("CLOTHO_DIFF_GRPC_URL", "http://localhost:50055"),
        merge_queue_grpc_url: env_or("CLOTHO_MERGE_QUEUE_GRPC_URL", "http://localhost:50053"),
        agent_gateway_url: env_or("CLOTHO_AGENT_GATEWAY_URL", "http://localhost:8090"),
        agent_admin_token: env_or("CLOTHO_AGENT_ADMIN_TOKEN", ""),
        compute_grpc_url: env_or("CLOTHO_COMPUTE_GRPC_URL", "http://localhost:50057"),
        webhook_secret: env_or("CLOTHO_WEBHOOK_SECRET", ""),
        webhook_url: env_or(
            "CLOTHO_WEBHOOK_URL",
            "http://clotho-api-gateway:8080/api/v1/webhooks/forgejo",
        ),
        web_url: env_or("CLOTHO_WEB_URL", "http://localhost:3100"),
        compute_provider: env_or("CLOTHO_COMPUTE_PROVIDER", "daytona"),
        compute_default_image: env_or("CLOTHO_COMPUTE_SNAPSHOT", "ubuntu:22.04"),
        actions_timeout_seconds: env_u32_or("CLOTHO_ACTIONS_TIMEOUT_SECONDS", 900),
        configured_providers: {
            // Env-only hints for fallback when clotho-compute is unreachable.
            // Live ListProviders + Clotho secret overlay are authoritative.
            // "Configured" must mean jobs can run — URL alone is not enough.
            let mut m = std::collections::HashMap::new();
            let daytona = env_truthy("CLOTHO_DAYTONA_CONFIGURED")
                || std::env::var("DAYTONA_API_KEY")
                    .map(|key| !key.trim().is_empty())
                    .unwrap_or(false);
            m.insert("daytona".into(), daytona);
            // Bridge URL does not imply upstream keys; leave false here so
            // overlay/live health own honesty for computesdk.
            m.insert("computesdk".into(), false);
            let box_cfg = env_truthy("CLOTHO_BOX_CONFIGURED")
                || std::env::var("BOX_API_KEY")
                    .map(|key| !key.trim().is_empty())
                    .unwrap_or(false);
            m.insert("box".into(), box_cfg);
            m
        },
        bootstrap_user_name: env_or("CLOTHO_BOOTSTRAP_USER_NAME", "clotho"),
        bootstrap_user_email: env_or("CLOTHO_BOOTSTRAP_USER_EMAIL", "admin@clotho.internal"),
        bootstrap_org_name: env_or("CLOTHO_BOOTSTRAP_ORG_NAME", "clotho"),
        bootstrap_org_display_name: env_or("CLOTHO_BOOTSTRAP_ORG_DISPLAY_NAME", "Clotho"),
        forgejo: ForgejoConfig {
            base_url: env_or("CLOTHO_FORGEJO_URL", "http://localhost:13000"),
            owner: env_or("CLOTHO_FORGEJO_OWNER", "clotho"),
            token,
        },
    };

    let pool = match std::env::var("CLOTHO_GATEWAY_DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => Some(clotho_api_gateway::init_db(&url).await?),
        _ => None,
    };

    let bootstrap = Bootstrap::from_config(&config);
    if let Some(ref p) = pool {
        control::ensure_bootstrap(p, &bootstrap)
            .await
            .map_err(|e| Error::Config(format!("{e}")))?;
    }

    let router = clotho_api_gateway::router_with_pool(config, pool, bootstrap)?;
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    tracing::info!(service = SERVICE, %http_addr, %grpc_addr, "listening");

    let http = async {
        axum::serve(listener, router)
            .await
            .map_err(std::io::Error::other)?;
        Ok::<(), Error>(())
    };
    let grpc = async {
        Server::builder()
            .add_service(
                health::HealthService::new(SERVICE, env!("CARGO_PKG_VERSION")).into_server(),
            )
            .serve(grpc_addr)
            .await?;
        Ok::<(), Error>(())
    };
    tokio::try_join!(http, grpc)?;
    Ok(())
}
