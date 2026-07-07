//! Shared foundation for all Clotho services: protobuf-generated types,
//! error types, tracing setup, and the common health-check service.

pub mod error;
pub mod health;
pub mod telemetry;

pub use error::Error;

/// Protobuf-generated types, namespaced to mirror `/proto`.
pub mod pb {
    pub mod health {
        pub mod v1 {
            tonic::include_proto!("clotho.health.v1");
        }
    }
}
