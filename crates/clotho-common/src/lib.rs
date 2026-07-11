//! Shared foundation for all Clotho services: protobuf-generated types,
//! error types, tracing setup, and the common health-check service.

pub mod error;
pub mod health;
pub mod lfs_pointer;
pub mod telemetry;

pub use error::Error;

/// Protobuf-generated types, namespaced to mirror `/proto`.
pub mod pb {
    pub mod health {
        pub mod v1 {
            tonic::include_proto!("clotho.health.v1");
        }
    }
    pub mod vcs {
        pub mod v1 {
            tonic::include_proto!("clotho.vcs.v1");
        }
    }
    pub mod storage {
        pub mod v1 {
            tonic::include_proto!("clotho.storage.v1");
        }
    }
    pub mod diff {
        pub mod v1 {
            tonic::include_proto!("clotho.diff.v1");
        }
    }
    pub mod mergequeue {
        pub mod v1 {
            tonic::include_proto!("clotho.mergequeue.v1");
        }
    }
    pub mod compute {
        pub mod v1 {
            tonic::include_proto!("clotho.compute.v1");
        }
    }
}
