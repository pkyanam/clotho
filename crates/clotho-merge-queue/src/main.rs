//! Multi-workspace reconciliation — serializes and rebases concurrent agent commits.

use clotho_common::{health, telemetry, Error};
use clotho_merge_queue::MergeQueueService;
use tonic::transport::Server;

const SERVICE: &str = "clotho-merge-queue";
const DEFAULT_PORT: u16 = 50053;

#[tokio::main]
async fn main() -> Result<(), Error> {
    telemetry::init();
    let addr = health::addr_from_env(DEFAULT_PORT)?;
    let vcs_grpc_url = std::env::var("CLOTHO_VCS_GRPC_URL")
        .unwrap_or_else(|_| "http://localhost:50051".to_string());
    let service = MergeQueueService::new(&vcs_grpc_url)?;
    tracing::info!(service = SERVICE, %addr, vcs = %vcs_grpc_url, "gRPC server listening");

    Server::builder()
        .add_service(health::HealthService::new(SERVICE, env!("CARGO_PKG_VERSION")).into_server())
        .add_service(service.into_server())
        .serve(addr)
        .await?;
    Ok(())
}
