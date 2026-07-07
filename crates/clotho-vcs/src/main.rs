//! Clotho VCS engine — wraps jj-lib; init, commit, checkpoint, restore, op-log query.

use clotho_common::{health, telemetry, Error};
use clotho_vcs::{VcsEngine, VcsService};
use tonic::transport::Server;

const SERVICE: &str = "clotho-vcs";
const DEFAULT_PORT: u16 = 50051;

#[tokio::main]
async fn main() -> Result<(), Error> {
    telemetry::init();
    let addr = health::addr_from_env(DEFAULT_PORT)?;

    let data_dir =
        std::env::var("CLOTHO_VCS_DATA_DIR").unwrap_or_else(|_| "./data/vcs-repos".to_string());
    // When set, backing bare git repos are created here as `<name>.git` — the
    // directory Forgejo scans as its repository root (docs/adr/0003).
    let git_repos_dir = std::env::var("CLOTHO_VCS_GIT_REPOS_DIR").ok();
    let engine = VcsEngine::with_git_root(&data_dir, git_repos_dir.as_deref())
        .map_err(|e| Error::Config(format!("failed to initialize engine at {data_dir}: {e}")))?;
    tracing::info!(service = SERVICE, %addr, data_dir, ?git_repos_dir, "gRPC server listening");

    Server::builder()
        .add_service(health::HealthService::new(SERVICE, env!("CARGO_PKG_VERSION")).into_server())
        .add_service(VcsService::new(engine).into_server())
        .serve(addr)
        .await?;
    Ok(())
}
