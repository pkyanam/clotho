//! Arachne storage engine — Xet-style chunk/xorb dedup against S3-compatible storage.

use clotho_common::{health, telemetry, Error};
use clotho_storage::{ArachneEngine, StorageConfig, StorageService};
use tonic::transport::Server;

const SERVICE: &str = "clotho-storage";
const DEFAULT_PORT: u16 = 50052;

#[tokio::main]
async fn main() -> Result<(), Error> {
    telemetry::init();
    let addr = health::addr_from_env(DEFAULT_PORT)?;

    let config = StorageConfig::from_env().map_err(|e| Error::Config(e.to_string()))?;
    let engine = ArachneEngine::new(&config)
        .await
        .map_err(|e| Error::Config(format!("failed to initialize engine: {e}")))?;
    tracing::info!(
        service = SERVICE,
        %addr,
        endpoint = %config.endpoint,
        bucket = %config.bucket,
        "gRPC server listening"
    );

    Server::builder()
        .add_service(health::HealthService::new(SERVICE, env!("CARGO_PKG_VERSION")).into_server())
        .add_service(StorageService::new(engine).into_server())
        .serve(addr)
        .await?;
    Ok(())
}
