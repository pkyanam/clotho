//! Clotho Stage 7 end-to-end demo (docs/prd.md §1 definition of done).
//!
//! One command, reproducible from a clean `docker compose up`. It drives the
//! whole prototype through Clotho's real APIs — the "small shared client" the
//! Stage 7 handoff calls for, rather than ad-hoc tooling:
//!
//!   1. two simulated agent sessions push concurrent commits → the merge-queue
//!      reconciles them into one graph with no human in the loop;
//!   2. a large binary uploaded twice (once modified) shows *measured*
//!      chunk-level dedup in the Arachne storage engine;
//!   3. a PR is opened for a human to review at :3100; and
//!   4. a push to that PR's branch triggers a CI job that runs on the real
//!      external sandbox provider (Daytona) and reports status back to the PR.
//!
//! Agents "commit" over the vcs gRPC API + merge-queue (the MCP surface has no
//! write path yet — a recorded Stage 8 candidate, docs/prd.md §5).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clotho_common::pb::mergequeue::v1::{
    merge_queue_client::MergeQueueClient, SubmitChangeRequest,
};
use clotho_common::pb::storage::v1::{
    storage_client::StorageClient, DownloadFileRequest, GetStorageStatsRequest, UploadFileRequest,
};
use clotho_common::pb::vcs::v1::{
    vcs_client::VcsClient, CommitRequest, FileChange, GetHeadsRequest,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type Result<T> = std::result::Result<T, anyhow::Error>;

struct Config {
    gateway: String,
    vcs: String,
    merge_queue: String,
    storage: String,
    forgejo: String,
    forgejo_user: String,
    forgejo_password: String,
    web: String,
    webhook_secret: String,
    dedup_mib: usize,
}

impl Config {
    fn from_env() -> Self {
        let env_or = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        Self {
            gateway: env_or("CLOTHO_DEMO_GATEWAY_URL", "http://localhost:8080"),
            vcs: env_or("CLOTHO_DEMO_VCS_GRPC_URL", "http://localhost:50051"),
            merge_queue: env_or("CLOTHO_DEMO_MERGE_QUEUE_GRPC_URL", "http://localhost:50053"),
            storage: env_or("CLOTHO_DEMO_STORAGE_GRPC_URL", "http://localhost:50052"),
            forgejo: env_or("CLOTHO_DEMO_FORGEJO_URL", "http://localhost:3000"),
            forgejo_user: env_or("CLOTHO_DEMO_FORGEJO_USER", "clotho"),
            forgejo_password: env_or("CLOTHO_DEMO_FORGEJO_PASSWORD", "clotho-dev"),
            web: env_or("CLOTHO_DEMO_WEB_URL", "http://localhost:3100"),
            webhook_secret: env_or("CLOTHO_WEBHOOK_SECRET", "clotho-webhook-dev"),
            dedup_mib: env_or("CLOTHO_DEMO_DEDUP_MIB", "64").parse().unwrap_or(64),
        }
    }
}

fn banner(step: &str, title: &str) {
    println!("\n\x1b[1m━━ {step}  {title}\x1b[0m");
}

fn note(msg: &str) {
    println!("   {msg}");
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env();
    let http = reqwest::Client::new();

    println!("\x1b[1mClotho — Stage 7 end-to-end demo\x1b[0m");
    note(&format!(
        "gateway {}  forgejo {}  web {}",
        cfg.gateway, cfg.forgejo, cfg.web
    ));

    // Preflight: the stack must be up.
    let health = http
        .get(format!("{}/healthz", cfg.gateway))
        .send()
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "gateway unreachable at {} ({e}); run `just dev` first",
                cfg.gateway
            )
        })?;
    anyhow::ensure!(health.status().is_success(), "gateway /healthz not ok");

    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let repo = format!("demo-{nanos}");

    // ── Provision ───────────────────────────────────────────────────────────
    banner("[0/4]", "provision a repo (vcs + Forgejo + CI webhook)");
    let created: serde_json::Value = http
        .post(format!("{}/api/v1/repos", cfg.gateway))
        .json(&serde_json::json!({ "name": repo }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let owner = created["owner"].as_str().unwrap_or("clotho").to_string();
    let initial = created["initial_commit_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    note(&format!(
        "created {owner}/{repo} (initial commit {})",
        short(&initial)
    ));

    merge_queue_demo(&cfg, &repo, &initial).await?;
    storage_demo(&cfg).await?;
    ci_and_pr_demo(&cfg, &http, &owner, &repo).await?;

    banner("[✓]", "demo complete");
    note(&format!("review the PR: {}/repos/{repo}/pulls/1", cfg.web));
    Ok(())
}

/// Two agents commit concurrently off the same base; the merge-queue serializes
/// and reconciles them onto `main` with no human in the loop (Stage 5).
async fn merge_queue_demo(cfg: &Config, repo: &str, base: &str) -> Result<()> {
    banner("[1/4]", "two agent sessions push concurrent commits");
    let mut vcs = VcsClient::connect(cfg.vcs.clone()).await?;

    let agent_commit = |file: &str, body: &str, who: &str| CommitRequest {
        repo: repo.to_string(),
        parent_commit_ids: vec![base.to_string()],
        files: vec![FileChange {
            path: file.to_string(),
            content: body.as_bytes().to_vec(),
            executable: false,
        }],
        deleted_paths: vec![],
        message: format!("{who}: add {file}"),
        author_name: who.to_string(),
        author_email: format!("{who}@agents.clotho.internal"),
    };

    // Both branch off `base` — genuinely concurrent (neither sees the other).
    let a = vcs
        .commit(agent_commit(
            "agent_a.txt",
            "written by agent A\n",
            "agent-a",
        ))
        .await?
        .into_inner();
    let b = vcs
        .commit(agent_commit(
            "agent_b.txt",
            "written by agent B\n",
            "agent-b",
        ))
        .await?
        .into_inner();
    note(&format!("agent-a committed {}", short(&a.commit_id)));
    note(&format!(
        "agent-b committed {} (sibling — off the same base)",
        short(&b.commit_id)
    ));

    let mut mq = MergeQueueClient::connect(cfg.merge_queue.clone()).await?;
    for (who, commit) in [("agent-a", &a.commit_id), ("agent-b", &b.commit_id)] {
        let out = mq
            .submit_change(SubmitChangeRequest {
                repo: repo.to_string(),
                commit_id: commit.clone(),
            })
            .await?
            .into_inner();
        let how = if out.fast_forwarded {
            "fast-forward"
        } else {
            "rebased onto main"
        };
        let conflict = if out.conflicted {
            " [conflict surfaced, not blocking]"
        } else {
            ""
        };
        note(&format!(
            "merge-queue landed {}'s change as {} ({how}){conflict}",
            who,
            short(&out.commit_id)
        ));
    }

    let heads = vcs
        .get_heads(GetHeadsRequest {
            repo: repo.to_string(),
        })
        .await?
        .into_inner();
    note(&format!(
        "reconciled into one graph — main is now {} (no human intervened)",
        short(&heads.main_commit_id)
    ));
    Ok(())
}

/// Upload a large incompressible file, then a near-duplicate; report the
/// engine's *measured* chunk-level dedup and verify byte-identical download.
async fn storage_demo(cfg: &Config) -> Result<()> {
    banner("[2/4]", "measured chunk-level storage dedup");
    let mut storage = StorageClient::connect(cfg.storage.clone())
        .await?
        .max_decoding_message_size(64 * 1024 * 1024);

    let size = cfg.dedup_mib * 1024 * 1024;
    let original = pseudo_random(size, 0xC10704);
    note(&format!(
        "uploading a {} MiB incompressible file…",
        cfg.dedup_mib
    ));
    let up1 = upload(&mut storage, &original).await?;
    note(&format!(
        "  first upload: {} new, {} deduped (chunks: {} new / {} dup)",
        human(up1.new_bytes),
        human(up1.deduped_bytes),
        up1.new_chunks,
        up1.deduped_chunks
    ));

    // Near-duplicate: overwrite 64 KiB and insert 1 KiB (shifting the tail) —
    // content-defined chunking should dedup everything but the touched region.
    let mut modified = original.clone();
    let at = size / 2;
    modified[at..at + 64 * 1024].fill(0xAB);
    modified.splice(at..at, std::iter::repeat_n(0xCD, 1024));

    note("uploading a near-duplicate (64 KiB overwrite + 1 KiB insertion)…");
    let up2 = upload(&mut storage, &modified).await?;
    let pct = up2.stored_bytes_written as f64 / up2.file_size as f64 * 100.0;
    note(&format!(
        "  second upload: {} new, {} deduped; wrote {} to the store — {pct:.3}% of the file",
        human(up2.new_bytes),
        human(up2.deduped_bytes),
        human(up2.stored_bytes_written),
    ));

    // Ground truth + byte-identical reconstruction.
    let stats = storage
        .get_storage_stats(GetStorageStatsRequest {})
        .await?
        .into_inner();
    note(&format!(
        "  store now holds {} across {} xorbs + {} shards",
        human(stats.total_bytes),
        stats.xorb_count,
        stats.shard_count
    ));
    let back = download(&mut storage, &up2.file_hash).await?;
    anyhow::ensure!(back == modified, "reconstruction was not byte-identical!");
    note("  reconstruction verified byte-identical ✓");
    Ok(())
}

/// Commit a check to a branch through Forgejo (a real push), open a PR, and let
/// the push-triggered CI job run on the sandbox provider and report status.
async fn ci_and_pr_demo(
    cfg: &Config,
    http: &reqwest::Client,
    owner: &str,
    repo: &str,
) -> Result<()> {
    banner(
        "[3/4]",
        "push → CI on the sandbox provider → status on the PR",
    );
    let api = format!("{}/api/v1/repos/{owner}/{repo}", cfg.forgejo);
    let auth =
        |b: reqwest::RequestBuilder| b.basic_auth(&cfg.forgejo_user, Some(&cfg.forgejo_password));

    // Branch `ci-demo` at main's tip.
    auth(
        http.post(format!("{api}/branches"))
            .json(&serde_json::json!({
                "new_branch_name": "ci-demo",
                "old_ref_name": "main",
            })),
    )
    .send()
    .await?
    .error_for_status()?;

    // Commit a real check through Forgejo (fires the push webhook → CI).
    let ci_script = "#!/bin/sh\nset -e\necho \"clotho CI: checking out $(git rev-parse --short HEAD)\"\nls -la\necho \"all checks passed\"\n";
    let commit_resp: serde_json::Value = auth(
        http.post(format!("{api}/contents/.clotho/ci.sh"))
            .json(&serde_json::json!({
                "content": base64(ci_script.as_bytes()),
                "message": "add clotho CI check",
                "branch": "ci-demo",
            })),
    )
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;
    let head_sha = commit_resp["commit"]["sha"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    note(&format!(
        "pushed .clotho/ci.sh to ci-demo ({})",
        short(&head_sha)
    ));

    // Open the PR for review.
    let pr: serde_json::Value = auth(http.post(format!("{api}/pulls")).json(&serde_json::json!({
        "title": "add CI check",
        "head": "ci-demo",
        "base": "main",
    })))
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;
    let pr_number = pr["number"].as_i64().unwrap_or(1);
    note(&format!(
        "opened PR #{pr_number}: {}/repos/{repo}/pulls/{pr_number}",
        cfg.web
    ));

    // The registered webhook should already have fired. If no status shows up
    // shortly, deliver the (correctly signed) push event ourselves so the CI
    // leg runs regardless of Forgejo's event wiring — same gateway code path.
    if wait_for_status(cfg, http, owner, repo, &head_sha, Duration::from_secs(20))
        .await?
        .is_none()
    {
        note("no status yet — delivering the signed push webhook directly…");
        deliver_webhook(cfg, http, owner, repo, &head_sha).await?;
    }

    banner("[4/4]", "waiting for CI to report back");
    match wait_for_status(cfg, http, owner, repo, &head_sha, Duration::from_secs(300)).await? {
        Some((state, desc)) => {
            note(&format!("CI reported: \x1b[1m{state}\x1b[0m — {desc}"));
            if state == "error" {
                note("(a compute 'error' usually means DAYTONA_API_KEY is unset — set it in .env)");
            }
        }
        None => note("CI did not report a final status within the timeout"),
    }
    Ok(())
}

/// Poll Forgejo's combined commit status until it leaves `pending`.
async fn wait_for_status(
    cfg: &Config,
    http: &reqwest::Client,
    owner: &str,
    repo: &str,
    sha: &str,
    timeout: Duration,
) -> Result<Option<(String, String)>> {
    let url = format!(
        "{}/api/v1/repos/{owner}/{repo}/commits/{sha}/status",
        cfg.forgejo
    );
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let resp = http
            .get(&url)
            .basic_auth(&cfg.forgejo_user, Some(&cfg.forgejo_password))
            .send()
            .await?;
        if resp.status().is_success() {
            let v: serde_json::Value = resp.json().await?;
            let state = v["state"].as_str().unwrap_or("").to_string();
            if !state.is_empty() && state != "pending" {
                let desc = v["statuses"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|s| s["description"].as_str())
                    .unwrap_or("")
                    .to_string();
                return Ok(Some((state, desc)));
            }
        }
        if std::time::Instant::now() >= deadline {
            return Ok(None);
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// Deliver a minimal, correctly-signed push webhook to the gateway (fallback).
async fn deliver_webhook(
    cfg: &Config,
    http: &reqwest::Client,
    _owner: &str,
    repo: &str,
    sha: &str,
) -> Result<()> {
    let body = serde_json::to_vec(&serde_json::json!({
        "after": sha,
        "ref": "refs/heads/ci-demo",
        "repository": { "name": repo },
    }))?;
    let mut mac = Hmac::<Sha256>::new_from_slice(cfg.webhook_secret.as_bytes())?;
    mac.update(&body);
    let sig = hex(&mac.finalize().into_bytes());
    http.post(format!("{}/api/v1/webhooks/forgejo", cfg.gateway))
        .header("X-Gitea-Event", "push")
        .header("X-Gitea-Signature", sig)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

// ── storage streaming helpers ────────────────────────────────────────────────

async fn upload(
    storage: &mut StorageClient<tonic::transport::Channel>,
    data: &[u8],
) -> Result<clotho_common::pb::storage::v1::UploadFileResponse> {
    let blocks: Vec<UploadFileRequest> = data
        .chunks(4 * 1024 * 1024)
        .map(|c| UploadFileRequest { data: c.to_vec() })
        .collect();
    let resp = storage
        .upload_file(tokio_stream::iter(blocks))
        .await?
        .into_inner();
    Ok(resp)
}

async fn download(
    storage: &mut StorageClient<tonic::transport::Channel>,
    file_hash: &str,
) -> Result<Vec<u8>> {
    let mut stream = storage
        .download_file(DownloadFileRequest {
            file_hash: file_hash.to_string(),
        })
        .await?
        .into_inner();
    let mut out = Vec::new();
    while let Some(chunk) = stream.message().await? {
        out.extend_from_slice(&chunk.data);
    }
    Ok(out)
}

// ── small utilities ──────────────────────────────────────────────────────────

/// Deterministic incompressible-ish bytes via xorshift (no rand dependency).
fn pseudo_random(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.extend_from_slice(&state.to_le_bytes());
    }
    out.truncate(len);
    out
}

fn short(id: &str) -> String {
    id.chars().take(12).collect()
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.2} {}", UNITS[u])
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
