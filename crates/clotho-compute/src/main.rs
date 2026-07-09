//! Clotho Compute service — the CCI over gRPC (docs/prd.md §5 Stage 7/12).
//!
//! Builds a capability-aware [`ProviderRegistry`] from the environment
//! (Daytona direct, optional ComputeSDK bridge, Box stub). With no provider
//! credentials the gRPC surface stays up; jobs fail with FAILED_PRECONDITION
//! (docs/adr/0008, docs/adr/0013).

use std::sync::Arc;

use clotho_common::{health, telemetry, Error};
use clotho_compute::{registry_from_env, ComputeService};
use tonic::transport::Server;

const SERVICE: &str = "clotho-compute";
const DEFAULT_PORT: u16 = 50057;

#[tokio::main]
async fn main() -> Result<(), Error> {
    telemetry::init();
    let addr = health::addr_from_env(DEFAULT_PORT)?;

    let registry = Arc::new(registry_from_env());
    let infos = registry.list_infos();
    tracing::info!(
        service = SERVICE,
        %addr,
        default = registry.default_id(),
        providers = infos.len(),
        "gRPC server listening"
    );
    for (d, enabled) in &infos {
        tracing::info!(
            id = %d.id,
            kind = d.kind.as_str(),
            configured = d.configured,
            enabled,
            "registered compute provider"
        );
    }

    let service = ComputeService::new(registry);
    Server::builder()
        .add_service(health::HealthService::new(SERVICE, env!("CARGO_PKG_VERSION")).into_server())
        .add_service(service.into_server())
        .serve(addr)
        .await?;
    Ok(())
}
