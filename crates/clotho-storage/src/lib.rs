//! Arachne storage engine — Xet-style chunk/xorb dedup against S3-compatible
//! storage. See `engine.rs` for the architecture.

pub mod engine;
pub mod service;

pub use engine::{ArachneEngine, EngineError, StorageConfig, StorageStats, UploadOutcome};
pub use service::StorageService;
