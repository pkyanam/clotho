use tracing_subscriber::EnvFilter;

/// Initialize tracing for a Clotho service. Respects `RUST_LOG`; defaults to
/// `info` when unset. Safe to call once per process.
pub fn init() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}
