//! The common health-check service every Clotho service exposes, plus a
//! small helper to run a service that (for now) serves only health checks.

use std::net::SocketAddr;

use tonic::{transport::Server, Request, Response, Status};

use crate::pb::health::v1::{
    health_check_response::ServingStatus,
    health_server::{Health, HealthServer},
    HealthCheckRequest, HealthCheckResponse,
};
use crate::Error;

/// A health service that reports SERVING along with the service's name and
/// crate version.
#[derive(Debug, Clone)]
pub struct HealthService {
    service_name: &'static str,
    version: &'static str,
}

impl HealthService {
    pub fn new(service_name: &'static str, version: &'static str) -> Self {
        Self {
            service_name,
            version,
        }
    }

    pub fn into_server(self) -> HealthServer<Self> {
        HealthServer::new(self)
    }
}

#[tonic::async_trait]
impl Health for HealthService {
    async fn check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            status: ServingStatus::Serving as i32,
            service_name: self.service_name.to_string(),
            version: self.version.to_string(),
        }))
    }
}

/// Resolve the gRPC bind address from `CLOTHO_GRPC_ADDR`, falling back to
/// `0.0.0.0:<default_port>`.
pub fn addr_from_env(default_port: u16) -> Result<SocketAddr, Error> {
    match std::env::var("CLOTHO_GRPC_ADDR") {
        Ok(raw) => raw
            .parse()
            .map_err(|e| Error::Config(format!("CLOTHO_GRPC_ADDR {raw:?}: {e}"))),
        Err(_) => Ok(SocketAddr::from(([0, 0, 0, 0], default_port))),
    }
}

/// Serve a bare health-check gRPC server. Service crates start from this and
/// add their real services alongside as they're implemented.
pub async fn serve(
    service_name: &'static str,
    version: &'static str,
    addr: SocketAddr,
) -> Result<(), Error> {
    tracing::info!(service = service_name, %addr, "gRPC server listening");
    Server::builder()
        .add_service(HealthService::new(service_name, version).into_server())
        .serve(addr)
        .await?;
    Ok(())
}
