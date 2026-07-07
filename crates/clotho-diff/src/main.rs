//! Structured diff engine — tree-sitter symbol-level diffs for humans and agents.

use clotho_common::{health, telemetry, Error};
use clotho_diff::DiffService;
use tonic::transport::Server;

const SERVICE: &str = "clotho-diff";
const DEFAULT_PORT: u16 = 50055;

#[tokio::main]
async fn main() -> Result<(), Error> {
    telemetry::init();
    let addr = health::addr_from_env(DEFAULT_PORT)?;
    tracing::info!(service = SERVICE, %addr, "gRPC server listening");

    Server::builder()
        .add_service(health::HealthService::new(SERVICE, env!("CARGO_PKG_VERSION")).into_server())
        .add_service(DiffService.into_server())
        .serve(addr)
        .await?;
    Ok(())
}
