//! Agent interface — MCP server, agent identity and permission enforcement.
//!
//! Serves the MCP endpoint + admin REST on `CLOTHO_AGENT_HTTP_ADDR` (default
//! 0.0.0.0:8090) and the standard Clotho gRPC health check on the usual
//! service port.

use std::net::SocketAddr;

use clotho_agent_gateway::GatewayConfig;
use clotho_common::{health, telemetry, Error};
use tonic::transport::Server;

const SERVICE: &str = "clotho-agent-gateway";
const DEFAULT_PORT: u16 = 50054;

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    telemetry::init();
    let grpc_addr = health::addr_from_env(DEFAULT_PORT)?;
    let http_addr: SocketAddr = env_or("CLOTHO_AGENT_HTTP_ADDR", "0.0.0.0:8090")
        .parse()
        .map_err(|e| Error::Config(format!("CLOTHO_AGENT_HTTP_ADDR: {e}")))?;

    let admin_token = std::env::var("CLOTHO_AGENT_ADMIN_TOKEN")
        .map_err(|_| Error::Config("CLOTHO_AGENT_ADMIN_TOKEN is required".into()))?;
    let config = GatewayConfig {
        database_url: env_or(
            "CLOTHO_AGENT_DATABASE_URL",
            "postgres://clotho:clotho-dev@localhost:5432/clotho",
        ),
        vcs_grpc_url: env_or("CLOTHO_VCS_GRPC_URL", "http://localhost:50051"),
        diff_grpc_url: env_or("CLOTHO_DIFF_GRPC_URL", "http://localhost:50055"),
        merge_queue_grpc_url: env_or("CLOTHO_MERGE_QUEUE_GRPC_URL", "http://localhost:50053"),
        api_url: env_or("CLOTHO_API_URL", "http://localhost:8080"),
        admin_token,
    };

    let pool = clotho_agent_gateway::init_db(&config.database_url).await?;
    let router = clotho_agent_gateway::router(&config, pool)?;
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
