//! Edge API gateway — REST aggregation over Clotho services and Forgejo.
//!
//! Serves REST/JSON on `CLOTHO_GATEWAY_HTTP_ADDR` (default 0.0.0.0:8080) and
//! the standard Clotho gRPC health check on the usual service port.

use std::net::SocketAddr;

use clotho_api_gateway::forgejo::{ForgejoConfig, TokenSource};
use clotho_api_gateway::GatewayConfig;
use clotho_common::{health, telemetry, Error};
use tonic::transport::Server;

const SERVICE: &str = "clotho-api-gateway";
const DEFAULT_PORT: u16 = 50056;

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
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
        forgejo: ForgejoConfig {
            base_url: env_or("CLOTHO_FORGEJO_URL", "http://localhost:3000"),
            owner: env_or("CLOTHO_FORGEJO_OWNER", "clotho"),
            token,
        },
    };

    let router = clotho_api_gateway::router(config)?;
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
