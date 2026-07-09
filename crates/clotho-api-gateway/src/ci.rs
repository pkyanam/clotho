//! Push-triggered CI orchestration (docs/prd.md §5 Stage 7, docs/adr/0008).
//!
//! A Forgejo push webhook lands in `webhooks`, which spawns [`run`] as a
//! background task. Here we:
//!   1. mark the commit `pending` on the PR (Forgejo commit-status API),
//!   2. pull the repo's real git objects from clotho-vcs (`ExportRepoArchive`),
//!   3. hand them to clotho-compute (the CCI) to run a real check in an
//!      external sandbox, and
//!   4. report `success`/`failure` back to the commit.
//!
//! clotho-compute stays vendor- and collaboration-agnostic: it just runs a
//! script in a sandbox. Deciding *what* to run (and reporting status) is edge
//! concern and lives here, next to the Forgejo coupling.

use std::sync::Arc;

use clotho_common::pb::compute::v1::{JobFile, RunJobRequest};
use clotho_common::pb::vcs::v1::ExportRepoArchiveRequest;

use crate::actions::FinishedRun;
use crate::AppState;

const STATUS_CONTEXT: &str = "clotho-ci";
/// Where in the sandbox the repo archive is staged and unpacked.
const SANDBOX_WORKDIR: &str = "/tmp/clotho-ci";

struct CiOutput {
    exit_code: i32,
    logs: String,
    provider: String,
    sandbox_id: String,
}

/// Run CI for a pushed commit end to end, reporting status back to Forgejo.
/// Errors are logged and turned into a failed/errored commit status rather
/// than propagated — this runs detached from the webhook response.
pub async fn run(state: Arc<AppState>, repo: String, sha: String) {
    let run = state
        .actions
        .create_run(
            repo.clone(),
            sha.clone(),
            "main".into(),
            "push".into(),
            "forgejo".into(),
        )
        .await;
    run_existing(state, run.id, repo, sha).await;
}

/// Run CI for an already-created Clotho action run. Used by both push
/// webhooks and manual Actions starts.
pub async fn run_existing(state: Arc<AppState>, run_id: String, repo: String, sha: String) {
    let short = sha.get(..12).unwrap_or(&sha);
    let target_url = format!("{}/repos/{repo}/actions/{run_id}", state.web_url);

    state.actions.mark_running(&run_id).await;

    // Mark pending immediately so reviewers see CI is running.
    if let Err(e) = state
        .forgejo
        .set_commit_status(
            &repo,
            &sha,
            "pending",
            STATUS_CONTEXT,
            "running check in sandbox",
            &target_url,
        )
        .await
    {
        tracing::warn!(%repo, %sha, error = %e, "failed to set pending status");
    }

    let (state_str, conclusion, description, exit_code, logs, provider, sandbox_id) =
        match execute(&state, &repo, &sha).await {
            Ok(output) if output.exit_code == 0 => (
                "success",
                "success",
                "check passed".to_string(),
                Some(output.exit_code),
                output.logs,
                output.provider,
                output.sandbox_id,
            ),
            Ok(output) => {
                tracing::info!(%repo, %sha, exit_code = output.exit_code, "ci check failed");
                (
                    "failure",
                    "failure",
                    format!(
                        "check failed (exit {}): {}",
                        output.exit_code,
                        tail(&output.logs)
                    ),
                    Some(output.exit_code),
                    output.logs,
                    output.provider,
                    output.sandbox_id,
                )
            }
            Err(e) => {
                tracing::warn!(%repo, %sha, error = %e, "ci run errored");
                (
                    "error",
                    "error",
                    format!("ci error: {}", truncate(&e, 120)),
                    None,
                    e,
                    String::new(),
                    String::new(),
                )
            }
        };
    state
        .actions
        .finish_run(
            &run_id,
            FinishedRun {
                status: state_str.into(),
                conclusion: conclusion.into(),
                exit_code,
                logs,
                provider,
                sandbox_id,
            },
        )
        .await;
    tracing::info!(%repo, sha = %short, status = state_str, "ci finished");

    if let Err(e) = state
        .forgejo
        .set_commit_status(
            &repo,
            &sha,
            state_str,
            STATUS_CONTEXT,
            &description,
            &target_url,
        )
        .await
    {
        tracing::warn!(%repo, %sha, error = %e, "failed to set final status");
    }
}

/// Export the git objects, run the check in a sandbox, return result metadata.
async fn execute(state: &AppState, repo: &str, sha: &str) -> Result<CiOutput, String> {
    let archive = state
        .vcs
        .clone()
        .export_repo_archive(ExportRepoArchiveRequest {
            repo: repo.to_string(),
        })
        .await
        .map_err(|e| format!("export archive: {}", e.message()))?
        .into_inner();

    // Checkout the pushed commit when we have it; else the exported main tip.
    let checkout = if sha.is_empty() {
        archive.main_commit_id.clone()
    } else {
        sha.to_string()
    };

    // Route through the CCI registry by Actions config provider id — never
    // hard-code Daytona (docs/adr/0013). Empty provider_id lets the registry
    // pick a configured one-shot provider.
    let config = state.actions.config_for(repo).await;
    let provider_id = config.provider.trim().to_string();
    let snapshot = if config.default_image.trim().is_empty()
        || config.default_image == "ubuntu:22.04"
    {
        // Leave empty so the provider uses its own default snapshot when the
        // repo still has the generic gateway fallback image.
        String::new()
    } else {
        config.default_image.clone()
    };
    let timeout_secs = config.timeout_seconds;

    // Resolve provider credentials from Clotho secrets (docs/adr/0014).
    // Env-backed keys on clotho-compute remain a dev escape hatch.
    let mut provider_credentials = std::collections::HashMap::new();
    match crate::secrets::resolve_provider_api_key(state, repo, &provider_id).await {
        Ok(Some(api_key)) => {
            provider_credentials.insert("api_key".into(), api_key);
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(%repo, error = %e, "secret resolve failed; relying on compute env");
        }
    }

    let script = ci_script(repo, &checkout);
    let job = RunJobRequest {
        label: format!("{repo}@{}", checkout.get(..12).unwrap_or(&checkout)),
        snapshot,
        files: vec![JobFile {
            path: format!("{SANDBOX_WORKDIR}/repo.tar"),
            content: archive.tar,
        }],
        commands: vec![script],
        env: Default::default(),
        timeout_secs,
        provider_id,
        provider_credentials,
    };
    let result = state
        .compute
        .clone()
        .run_job(job)
        .await
        .map_err(|e| format!("compute: {}", e.message()))?
        .into_inner();
    Ok(CiOutput {
        exit_code: result.exit_code,
        logs: result.logs,
        provider: result.provider,
        sandbox_id: result.sandbox_id,
    })
}

/// The check script run inside the sandbox: unpack the git objects, clone,
/// check out the pushed commit, and run a repo-defined check (else a sensible
/// default probe). `repo` is validated `[a-z0-9-_]` and `sha` is validated hex
/// upstream, so neither can break out of the shell.
fn ci_script(repo: &str, sha: &str) -> String {
    format!(
        r#"set -eu
cd {workdir}
echo "=== clotho-ci: {repo}@{sha} ==="
tar xf repo.tar
rm -rf checkout
git clone --quiet repo.git checkout
cd checkout
{checkout_step}
if [ -f .clotho/ci.sh ]; then
  echo "--- running .clotho/ci.sh"; sh .clotho/ci.sh
elif [ -f Makefile ] || [ -f makefile ]; then
  echo "--- running make"; make
elif [ -f Cargo.toml ]; then
  echo "--- running cargo test"; cargo test
elif [ -f package.json ]; then
  echo "--- running npm test"; npm install --no-audit --no-fund >/dev/null 2>&1 || true; npm test
else
  echo "--- no CI check defined; clean checkout treated as success"
fi
"#,
        workdir = SANDBOX_WORKDIR,
        checkout_step = if sha.is_empty() {
            "echo '--- no commit to check out; using default branch'".to_string()
        } else {
            format!("git checkout --quiet {sha}")
        },
    )
}

/// Last ~160 chars of the logs, single-lined, for the status description.
fn tail(logs: &str) -> String {
    let flat = logs.replace('\n', " ");
    let trimmed = flat.trim();
    truncate(trimmed, 160)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}
