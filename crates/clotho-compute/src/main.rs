//! Clotho Compute service — the CCI over gRPC (docs/prd.md §5 Stage 7).
//!
//! Selects a [`ComputeProvider`] from the environment: Daytona when
//! `DAYTONA_API_KEY` is set, otherwise a disabled provider so the stack stays
//! healthy without a paid credential (jobs then fail with FAILED_PRECONDITION).

use std::sync::Arc;

use clotho_common::{health, telemetry, Error};
use clotho_compute::{ComputeProvider, ComputeService, DaytonaProvider, DisabledProvider};
use tonic::transport::Server;

const SERVICE: &str = "clotho-compute";
const DEFAULT_PORT: u16 = 50057;

#[tokio::main]
async fn main() -> Result<(), Error> {
    telemetry::init();
    let addr = health::addr_from_env(DEFAULT_PORT)?;

    let provider_name = std::env::var("CLOTHO_COMPUTE_PROVIDER")
        .unwrap_or_else(|_| "daytona".to_string())
        .to_lowercase();
    let provider: Arc<dyn ComputeProvider> = match provider_name.as_str() {
        "daytona" => match DaytonaProvider::from_env() {
            Some(p) => Arc::new(p),
            None => Arc::new(DisabledProvider::new(
                "DAYTONA_API_KEY not set; set it (in .env) to enable Daytona compute",
            )),
        },
        "disabled" | "none" => Arc::new(DisabledProvider::new(
            "compute explicitly disabled via CLOTHO_COMPUTE_PROVIDER",
        )),
        other => Arc::new(DisabledProvider::new(format!("unknown provider {other:?}"))),
    };
    tracing::info!(service = SERVICE, %addr, provider = provider.name(), "gRPC server listening");

    let service = ComputeService::new(provider);
    Server::builder()
        .add_service(health::HealthService::new(SERVICE, env!("CARGO_PKG_VERSION")).into_server())
        .add_service(service.into_server())
        .serve(addr)
        .await?;
    Ok(())
}
