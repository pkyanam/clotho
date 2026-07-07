//! Clotho VCS engine — wraps jj-lib; init, commit, checkpoint, restore, op-log query
//!
//! Currently a health-check stub — the real service lands per docs/prd.md §5.

use clotho_common::{health, telemetry, Error};

const SERVICE: &str = "clotho-vcs";
const DEFAULT_PORT: u16 = 50051;

#[tokio::main]
async fn main() -> Result<(), Error> {
    telemetry::init();
    let addr = health::addr_from_env(DEFAULT_PORT)?;
    health::serve(SERVICE, env!("CARGO_PKG_VERSION"), addr).await
}
