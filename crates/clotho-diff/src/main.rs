//! Structured diff engine — tree-sitter symbol-level diffs for humans and agents
//!
//! Currently a health-check stub — the real service lands per docs/prd.md §5.

use clotho_common::{health, telemetry, Error};

const SERVICE: &str = "clotho-diff";
const DEFAULT_PORT: u16 = 50055;

#[tokio::main]
async fn main() -> Result<(), Error> {
    telemetry::init();
    let addr = health::addr_from_env(DEFAULT_PORT)?;
    health::serve(SERVICE, env!("CARGO_PKG_VERSION"), addr).await
}
