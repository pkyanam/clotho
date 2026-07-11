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
use clotho_common::pb::vcs::v1::{ExportRepoArchiveRequest, GetFileRequest, ListFilesRequest};
use serde::Serialize;

use crate::actions::{FinishedRun, NewActionRun};
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
    let run = match state
        .actions
        .create_run(NewActionRun {
            repo: repo.clone(),
            commit_id: sha.clone(),
            branch: "main".into(),
            trigger: "push".into(),
            actor: "forgejo".into(),
            workflow: "ci".into(),
            release_version: String::new(),
            release_manifest_sha256: String::new(),
        })
        .await
    {
        Ok(run) => run,
        Err(error) => {
            tracing::error!(%repo, %sha, %error, "failed to persist push-triggered Action");
            return;
        }
    };
    run_existing(state, run.id, repo, sha).await;
}

/// Run CI for an already-created Clotho action run. Used by both push
/// webhooks and manual Actions starts.
pub async fn run_existing(state: Arc<AppState>, run_id: String, repo: String, sha: String) {
    let short = sha.get(..12).unwrap_or(&sha);
    let target_url = format!("{}/repos/{repo}/actions/{run_id}", state.web_url);

    let run_context = match state.actions.get_run(&repo, &run_id).await {
        Ok(run) => run,
        Err(error) => {
            tracing::error!(%repo, %run_id, %error, "cannot load Action provenance");
            return;
        }
    };
    let Some(worker_id) = state.actions.claim_run(&run_id).await else {
        return;
    };
    let (stop_heartbeat, mut heartbeat_stopped) = tokio::sync::oneshot::channel();
    let heartbeat_state = state.clone();
    let heartbeat_run_id = run_id.clone();
    let heartbeat_worker_id = worker_id.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        interval.tick().await;
        loop {
            tokio::select! {
                _ = &mut heartbeat_stopped => break,
                _ = interval.tick() => {
                    if !heartbeat_state.actions
                        .renew_run_lease(&heartbeat_run_id, &heartbeat_worker_id)
                        .await
                    {
                        break;
                    }
                }
            }
        }
    });

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
        match execute(&state, &repo, &sha, &run_context).await {
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
    let _ = stop_heartbeat.send(());
    state
        .actions
        .finish_run(
            &run_id,
            &worker_id,
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
async fn execute(
    state: &AppState,
    repo: &str,
    sha: &str,
    run: &crate::actions::ActionRun,
) -> Result<CiOutput, String> {
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
    let snapshot = job_snapshot(&config, &provider_id);
    let timeout_secs = config.timeout_seconds;

    // Resolve provider credentials from Clotho secrets (docs/adr/0014).
    // Env-backed keys on clotho-compute remain a dev escape hatch.
    let mut provider_credentials = std::collections::HashMap::new();
    if provider_id.eq_ignore_ascii_case("computesdk") {
        match crate::secrets::resolve_computesdk_credentials(state, repo).await {
            Ok(creds) => provider_credentials.extend(creds),
            Err(e) => {
                tracing::warn!(%repo, error = %e, "computesdk secret resolve failed");
            }
        }
    } else {
        match crate::secrets::resolve_provider_api_key(state, repo, &provider_id).await {
            Ok(Some(api_key)) => {
                provider_credentials.insert("api_key".into(), api_key);
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(%repo, error = %e, "secret resolve failed; relying on compute env");
            }
        }
    }

    let script = ci_script(repo, &checkout, &run.workflow);
    let mut job_files = vec![JobFile {
        path: format!("{SANDBOX_WORKDIR}/repo.tar"),
        content: archive.tar,
    }];
    job_files.extend(materialized_large_files(state, repo, &checkout).await?);
    let mut env = std::collections::HashMap::new();
    env.insert("CLOTHO_ACCELERATOR".into(), config.accelerator.clone());
    env.insert("CLOTHO_WORKFLOW".into(), run.workflow.clone());
    env.insert("CLOTHO_COMMIT_ID".into(), run.commit_id.clone());
    if !run.release_version.is_empty() {
        env.insert("CLOTHO_RELEASE_VERSION".into(), run.release_version.clone());
        env.insert(
            "CLOTHO_RELEASE_MANIFEST_SHA256".into(),
            run.release_manifest_sha256.clone(),
        );
        let pool = state
            .pool
            .as_ref()
            .ok_or_else(|| "release runtime requires the Clotho control plane".to_string())?;
        let repo_metadata = crate::control::get_repo_with_org(pool, repo)
            .await
            .map_err(|error| format!("resolve release runtime repository: {error}"))?
            .ok_or_else(|| format!("repository {repo:?} disappeared before Action dispatch"))?;
        let (manifest, release_env) = release_runtime(
            &format!("{}/{}", repo_metadata.org_name, repo),
            &repo_metadata.repo.kind,
            &run.release_version,
            &run.commit_id,
            &run.release_manifest_sha256,
        )?;
        job_files.push(manifest);
        env.extend(release_env);
    }
    if !config.gpu_types.is_empty() {
        env.insert("CLOTHO_GPU_TYPES".into(), config.gpu_types.join(","));
    }
    let job = RunJobRequest {
        label: format!("{repo}@{}", checkout.get(..12).unwrap_or(&checkout)),
        snapshot,
        files: job_files,
        commands: vec![script],
        env,
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

#[derive(Serialize)]
struct ReleaseRuntimeManifest<'a> {
    schema_version: u8,
    repo_id: &'a str,
    repo_kind: &'a str,
    version: &'a str,
    commit_id: &'a str,
    manifest_sha256: &'a str,
    artifact_root: &'static str,
    source_of_truth: &'static str,
}

fn release_runtime(
    repo_id: &str,
    repo_kind: &str,
    version: &str,
    commit_id: &str,
    manifest_sha256: &str,
) -> Result<(JobFile, std::collections::HashMap<String, String>), String> {
    let artifact_root = format!("{SANDBOX_WORKDIR}/checkout");
    let metadata_path = format!("{SANDBOX_WORKDIR}/release.json");
    let manifest = ReleaseRuntimeManifest {
        schema_version: 1,
        repo_id,
        repo_kind,
        version,
        commit_id,
        manifest_sha256,
        artifact_root: "/tmp/clotho-ci/checkout",
        source_of_truth: "clotho",
    };
    let content = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("encode release runtime manifest: {error}"))?;
    let mut env = std::collections::HashMap::from([
        ("CLOTHO_RELEASE_PATH".into(), artifact_root.clone()),
        ("CLOTHO_ARTIFACT_ROOT".into(), artifact_root),
        ("CLOTHO_RELEASE_METADATA".into(), metadata_path.clone()),
        ("CLOTHO_REPO_ID".into(), repo_id.into()),
        ("CLOTHO_REPO_KIND".into(), repo_kind.into()),
        (
            "CLOTHO_RELEASE_URI".into(),
            format!("clotho://{repo_id}@{version}"),
        ),
        ("HF_HUB_OFFLINE".into(), "1".into()),
    ]);
    if repo_kind == "model" {
        env.insert("TRANSFORMERS_OFFLINE".into(), "1".into());
    } else if repo_kind == "dataset" {
        env.insert("HF_DATASETS_OFFLINE".into(), "1".into());
    }
    Ok((
        JobFile {
            path: metadata_path,
            content,
        },
        env,
    ))
}

fn job_snapshot(config: &crate::actions::ActionsConfig, provider_id: &str) -> String {
    if config.accelerator == "gpu" && provider_id.eq_ignore_ascii_case("daytona") {
        // Daytona's supported GPU entry point is a provider-native snapshot;
        // keeping this translation at the CCI edge avoids vendor syntax in
        // repository workflow files.
        return "daytona-gpu".into();
    }
    if config.default_image.trim().is_empty() || config.default_image == "ubuntu:22.04" {
        // Leave empty so the provider uses its own default snapshot when the
        // repo still has the generic gateway fallback image.
        String::new()
    } else {
        config.default_image.clone()
    }
}

/// Ship Arachne payloads beside the bare-repo archive. Sandboxes may have no
/// route back to a local/private Clotho control plane, so CI materializes
/// pointers from these files after checkout instead of requiring credentials.
async fn materialized_large_files(
    state: &AppState,
    repo: &str,
    commit_id: &str,
) -> Result<Vec<JobFile>, String> {
    let mut vcs = state.vcs.clone();
    let tree = vcs
        .list_files(ListFilesRequest {
            repo: repo.to_string(),
            commit_id: commit_id.to_string(),
        })
        .await
        .map_err(|err| format!("list files for Arachne materialization: {}", err.message()))?
        .into_inner();
    let mut payloads = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in tree.files {
        let file = vcs
            .get_file(GetFileRequest {
                repo: repo.to_string(),
                commit_id: tree.commit_id.clone(),
                path: entry.path,
            })
            .await
            .map_err(|err| format!("read file for Arachne materialization: {}", err.message()))?
            .into_inner();
        let Ok(pointer) = clotho_common::lfs_pointer::LfsPointer::parse(&file.content) else {
            continue;
        };
        if !seen.insert(pointer.arachne_hash.clone()) {
            continue;
        }
        let payload = crate::arachne::materialize_pointer(state, &file.content)
            .await
            .map_err(|err| format!("materialize Arachne payload: {err}"))?
            .ok_or_else(|| "Arachne pointer unexpectedly parsed as ordinary blob".to_string())?;
        payloads.push(JobFile {
            path: format!("{SANDBOX_WORKDIR}/arachne/{}", pointer.arachne_hash),
            content: payload,
        });
    }
    Ok(payloads)
}

/// The check script run inside the sandbox: unpack the git objects, clone,
/// check out the pushed commit, and run a repo-defined check (else a sensible
/// default probe). `repo` is validated `[a-z0-9-_]` and `sha` is validated hex
/// upstream, so neither can break out of the shell.
fn ci_script(repo: &str, sha: &str, workflow: &str) -> String {
    let workflow_step = match workflow {
        "evaluate" => workflow_script("evaluate"),
        "inference" => workflow_script("inference"),
        "benchmark" => workflow_script("benchmark"),
        _ => r#"if [ -f .clotho/ci.sh ]; then
  echo "--- running .clotho/ci.sh"; sh .clotho/ci.sh
elif [ -f Makefile ] || [ -f makefile ]; then
  echo "--- running make"; make
elif [ -f Cargo.toml ]; then
  echo "--- running cargo test"; cargo test
elif [ -f package.json ]; then
  echo "--- running npm test"; npm install --no-audit --no-fund >/dev/null 2>&1 || true; npm test
else
  echo "--- no CI check defined; clean checkout treated as success"
fi"#
        .to_string(),
    };
    format!(
        r#"set -eu
cd {workdir}
echo "=== clotho-{workflow}: {repo}@{sha} ==="
tar xf repo.tar
rm -rf checkout
git clone --quiet repo.git checkout
cd checkout
{checkout_step}
# Hosted sandboxes may not be able to reach Clotho. Replace Arachne pointer
# blobs from the payload bundle shipped with this job before running checks.
find . -type f -size -1k -exec sh -c '
  for path do
    hash=$(sed -n "s/^x-clotho-arachne-hash //p" "$path")
    if [ -n "$hash" ] && [ -f "../arachne/$hash" ]; then
      cp "../arachne/$hash" "$path"
    fi
  done
' sh {{}} +
{workflow_step}
"#,
        workdir = SANDBOX_WORKDIR,
        workflow = workflow,
        workflow_step = workflow_step,
        checkout_step = if sha.is_empty() {
            "echo '--- no commit to check out; using default branch'".to_string()
        } else {
            format!("git checkout --quiet {sha}")
        },
    )
}

fn workflow_script(workflow: &str) -> String {
    format!(
        r#"if [ ! -f .clotho/{workflow}.sh ]; then
  echo "missing required .clotho/{workflow}.sh for release-pinned {workflow} workflow" >&2
  exit 64
fi
echo "--- running .clotho/{workflow}.sh"
sh .clotho/{workflow}.sh"#
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

#[cfg(test)]
mod tests {
    use super::{ci_script, job_snapshot, release_runtime};
    use crate::actions::ActionsConfig;

    fn config(accelerator: &str, image: &str) -> ActionsConfig {
        ActionsConfig {
            enabled: true,
            provider: "daytona".into(),
            default_image: image.into(),
            timeout_seconds: 900,
            accelerator: accelerator.into(),
            gpu_types: vec!["H100".into()],
        }
    }

    #[test]
    fn daytona_gpu_policy_maps_to_provider_snapshot() {
        assert_eq!(
            job_snapshot(&config("gpu", "ubuntu:22.04"), "daytona"),
            "daytona-gpu"
        );
    }

    #[test]
    fn cpu_policy_keeps_provider_default_or_explicit_image() {
        assert_eq!(job_snapshot(&config("cpu", "ubuntu:22.04"), "daytona"), "");
        assert_eq!(
            job_snapshot(&config("cpu", "custom-snapshot"), "daytona"),
            "custom-snapshot"
        );
    }

    #[test]
    fn release_workflows_use_explicit_fail_closed_scripts() {
        let evaluation = ci_script("model", "abc123", "evaluate");
        assert!(evaluation.contains(".clotho/evaluate.sh"));
        assert!(evaluation.contains("exit 64"));
        assert!(!evaluation.contains("clean checkout treated as success"));

        let ci = ci_script("model", "abc123", "ci");
        assert!(ci.contains(".clotho/ci.sh"));
        assert!(ci.contains("clean checkout treated as success"));
    }

    #[test]
    fn model_releases_are_self_describing_offline_runtimes() {
        let (file, env) =
            release_runtime("clotho/llm", "model", "v1.0.0", "abc123", "digest123").unwrap();
        assert_eq!(file.path, "/tmp/clotho-ci/release.json");
        let manifest: serde_json::Value = serde_json::from_slice(&file.content).unwrap();
        assert_eq!(manifest["repo_id"], "clotho/llm");
        assert_eq!(manifest["source_of_truth"], "clotho");
        assert_eq!(manifest["manifest_sha256"], "digest123");
        assert_eq!(env["CLOTHO_RELEASE_PATH"], "/tmp/clotho-ci/checkout");
        assert_eq!(env["CLOTHO_RELEASE_URI"], "clotho://clotho/llm@v1.0.0");
        assert_eq!(env["HF_HUB_OFFLINE"], "1");
        assert_eq!(env["TRANSFORMERS_OFFLINE"], "1");
        assert!(!env.contains_key("HF_DATASETS_OFFLINE"));
    }
}
