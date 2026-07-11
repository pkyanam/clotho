//! `clotho` CLI: thin human-facing client over the api-gateway REST edge.
//! Never shells out to git or jj; local files are read and sent to the gateway.

mod args;
mod client;

use std::path::Path;

use anyhow::{bail, Context, Result};
use base64::Engine;
use serde_json::{json, Value};

use crate::args::{
    require_one, require_two, strip_globals, take_flag, take_option, take_repeated,
    take_repeated_str,
};
use crate::client::{emit, first_line, request_json, request_value, short, Config};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let (api_opt, json, token_opt) = strip_globals(&mut args);
    let api_url = api_opt
        .or_else(|| std::env::var("CLOTHO_API_URL").ok())
        .unwrap_or_else(|| "http://localhost:8080".into());
    let token = token_opt.or_else(|| std::env::var("CLOTHO_TOKEN").ok());
    let config = Config::from_env_and_args(api_url, json, token);

    let Some(command) = args.first().cloned() else {
        usage();
        return Ok(());
    };
    args.remove(0);

    match command.as_str() {
        "auth" => cmd_auth(&config, args).await,
        // Grouped commands (Stage 15).
        "repo" => cmd_repo(&config, args).await,
        "issue" => cmd_issue(&config, args).await,
        "label" => cmd_label(&config, args).await,
        "milestone" => cmd_milestone(&config, args).await,
        "notification" => cmd_notification(&config, args).await,
        "pr" => cmd_pr(&config, args).await,
        "actions" => cmd_actions(&config, args).await,
        "provider" => cmd_provider(&config, args).await,
        "secret" => cmd_secret(&config, args).await,
        "org" => cmd_org(&config, args).await,
        "activity" => cmd_activity(&config, args).await,
        "agent" => cmd_agent(&config, args).await,
        // Backward-compatible Stage 8 aliases.
        "init" => cmd_repo(&config, prepend("init", args)).await,
        "status" => cmd_repo(&config, prepend("status", args)).await,
        "log" => cmd_repo(&config, prepend("log", args)).await,
        "commit" => cmd_repo(&config, prepend("commit", args)).await,
        "submit" => cmd_repo(&config, prepend("submit", args)).await,
        "help" | "-h" | "--help" => {
            usage();
            Ok(())
        }
        other => bail!("unknown command {other:?}; run `clotho help`"),
    }
}

fn prepend(cmd: &str, mut args: Vec<String>) -> Vec<String> {
    args.insert(0, cmd.to_string());
    args
}

// ---------------------------------------------------------------------------
// auth
// ---------------------------------------------------------------------------

async fn cmd_auth(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho auth <whoami|token> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "whoami" => {
            let body: Value =
                request_json(config, reqwest::Method::GET, "/api/v1/me", None).await?;
            emit(config, &body, || {
                let user = &body["user"];
                println!(
                    "{} <{}> ({})",
                    user["name"].as_str().unwrap_or("?"),
                    user["email"].as_str().unwrap_or(""),
                    user["id"].as_str().unwrap_or("")
                );
            })
        }
        "token" => {
            let Some(action) = args.first().cloned() else {
                bail!("usage: clotho auth token <create|list|revoke> ...");
            };
            args.remove(0);
            match action.as_str() {
                "create" => {
                    let name = take_option(&mut args, "--name").unwrap_or_default();
                    let body = request_json(
                        config,
                        reqwest::Method::POST,
                        "/api/v1/tokens",
                        Some(json!({ "name": name })),
                    )
                    .await?;
                    emit(config, &body, || {
                        println!(
                            "token {} (prefix {}) — save this value, it is shown once:\n{}",
                            body["id"].as_str().unwrap_or("?"),
                            body["token_prefix"].as_str().unwrap_or(""),
                            body["token"].as_str().unwrap_or("")
                        );
                    })
                }
                "list" => {
                    let body: Value =
                        request_json(config, reqwest::Method::GET, "/api/v1/tokens", None).await?;
                    emit(config, &body, || {
                        for t in body["tokens"].as_array().into_iter().flatten() {
                            println!(
                                "{}  {}  {}",
                                t["id"].as_str().unwrap_or("?"),
                                t["token_prefix"].as_str().unwrap_or(""),
                                t["name"].as_str().unwrap_or("")
                            );
                        }
                    })
                }
                "revoke" => {
                    let id = require_one(&args, "clotho auth token revoke <id>")?;
                    request_value(
                        config,
                        reqwest::Method::DELETE,
                        &format!("/api/v1/tokens/{id}"),
                        None,
                    )
                    .await?;
                    emit(config, &json!(null), || println!("revoked {id}"))
                }
                other => bail!("unknown auth token subcommand {other:?}"),
            }
        }
        other => bail!("unknown auth subcommand {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// repo
// ---------------------------------------------------------------------------

async fn cmd_repo(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho repo <init|list|status|log|commit|submit|tree|artifacts|preview|import-hf|get> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "init" | "create" => {
            if args.is_empty() {
                bail!("usage: clotho repo init <name> [--kind code|model|dataset] [--description <text>] [--visibility public|private|internal] [--large-file-threshold <bytes>] [--network public|tailscale] [--network-tag tag:name]...");
            }
            let name = args.remove(0);
            let kind = take_option(&mut args, "--kind").unwrap_or_else(|| "code".into());
            let description = take_option(&mut args, "--description").unwrap_or_default();
            let visibility =
                take_option(&mut args, "--visibility").unwrap_or_else(|| "public".into());
            let threshold = take_option(&mut args, "--large-file-threshold")
                .map(|value| {
                    value
                        .parse::<i64>()
                        .context("--large-file-threshold must be an integer")
                })
                .transpose()?;
            let network_mode =
                take_option(&mut args, "--network").unwrap_or_else(|| "public".into());
            let network_tags = take_repeated(&mut args, "--network-tag");
            if !args.is_empty() {
                bail!("unrecognized repo init arguments: {}", args.join(" "));
            }
            let body = request_value(
                config,
                reqwest::Method::POST,
                "/api/v1/repos",
                Some(json!({
                    "name": name,
                    "description": description,
                    "visibility": visibility,
                    "kind": kind,
                    "large_file_threshold_bytes": threshold,
                    "network_mode": network_mode,
                    "network_tags": network_tags,
                })),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "created {}/{} ({}) at {}",
                    body["owner"].as_str().unwrap_or("?"),
                    body["name"].as_str().unwrap_or(&name),
                    body["kind"].as_str().unwrap_or("code"),
                    body["initial_commit_id"].as_str().unwrap_or("")
                );
            })
        }
        "list" => {
            let body: Value =
                request_json(config, reqwest::Method::GET, "/api/v1/repos", None).await?;
            emit(config, &body, || {
                for repo in body["repos"].as_array().into_iter().flatten() {
                    println!(
                        "{}  {}  issues={} prs={}",
                        repo["name"].as_str().unwrap_or("?"),
                        repo["visibility"].as_str().unwrap_or(""),
                        repo["open_issues_count"].as_i64().unwrap_or(0),
                        repo["open_pr_counter"].as_i64().unwrap_or(0)
                    );
                }
            })
        }
        "status" | "get" => {
            let repo = require_one(&args, "clotho repo status <repo>")?;
            let detail: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}"),
                None,
            )
            .await?;
            let tree: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/tree"),
                None,
            )
            .await?;
            let out = json!({ "repo": detail, "tree": tree });
            emit(config, &out, || {
                let files = tree["files"].as_array().map_or(0, Vec::len);
                println!(
                    "{}/{}",
                    detail["owner"].as_str().unwrap_or("?"),
                    detail["name"].as_str().unwrap_or(&repo)
                );
                println!(
                    "main {}",
                    short(detail["main_commit_id"].as_str().unwrap_or(""))
                );
                println!("heads {}", detail["heads"].as_array().map_or(0, Vec::len));
                println!("files {files}");
            })
        }
        "log" => {
            let repo = require_one(&args, "clotho repo log <repo>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/commits?limit=20"),
                None,
            )
            .await?;
            emit(config, &body, || {
                for commit in body["commits"].as_array().into_iter().flatten() {
                    println!(
                        "{} {} {} {}",
                        short(commit["commit_id"].as_str().unwrap_or("")),
                        commit["timestamp_millis"].as_i64().unwrap_or(0),
                        commit["author_name"].as_str().unwrap_or(""),
                        first_line(commit["description"].as_str().unwrap_or(""))
                    );
                }
            })
        }
        "tree" => {
            let repo = require_one(&args, "clotho repo tree <repo>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/tree"),
                None,
            )
            .await?;
            emit(config, &body, || {
                for f in body["files"].as_array().into_iter().flatten() {
                    println!(
                        "{}  {}B{}",
                        f["path"].as_str().unwrap_or("?"),
                        f["size_bytes"].as_u64().unwrap_or(0),
                        if f["conflicted"].as_bool().unwrap_or(false) {
                            "  conflicted"
                        } else {
                            ""
                        }
                    );
                }
            })
        }
        "artifacts" => {
            let repo = require_one(&args, "clotho repo artifacts <repo>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/artifacts"),
                None,
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "{}  {} files  {} bytes  {}",
                    body["kind"].as_str().unwrap_or("repository"),
                    body["total_files"].as_u64().unwrap_or(0),
                    body["total_bytes"].as_u64().unwrap_or(0),
                    if body["readiness"]["ready"].as_bool().unwrap_or(false) {
                        "publishable"
                    } else {
                        "needs attention"
                    }
                );
                for artifact in body["artifacts"].as_array().into_iter().flatten() {
                    println!(
                        "{}  {}  {}  {}B{}",
                        artifact["role"].as_str().unwrap_or("other"),
                        artifact["format"].as_str().unwrap_or("unknown"),
                        artifact["path"].as_str().unwrap_or("?"),
                        artifact["size_bytes"].as_u64().unwrap_or(0),
                        if artifact["storage"].as_str() == Some("arachne") {
                            "  Arachne"
                        } else {
                            ""
                        }
                    );
                }
                for warning in body["readiness"]["warnings"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    println!("warning: {}", warning.as_str().unwrap_or(""));
                }
                if body["metadata"]
                    .as_object()
                    .is_some_and(|value| !value.is_empty())
                {
                    println!("metadata: {}", body["metadata"]);
                    println!("sources: {}", body["metadata_sources"]);
                }
            })
        }
        "preview" => {
            if args.len() < 2 {
                bail!("usage: clotho repo preview <repo> <path> [--limit 1..100]");
            }
            let repo = args.remove(0);
            let path = args.remove(0);
            let limit = take_option(&mut args, "--limit")
                .map(|value| value.parse::<u32>().context("--limit must be an integer"))
                .transpose()?
                .unwrap_or(25);
            if !args.is_empty() {
                bail!("unrecognized repo preview arguments: {}", args.join(" "));
            }
            let mut query = reqwest::Url::parse("http://clotho.invalid/")?;
            query
                .query_pairs_mut()
                .append_pair("path", &path)
                .append_pair("limit", &limit.to_string());
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!(
                    "/api/v1/repos/{repo}/artifacts/preview?{}",
                    query.query().unwrap_or_default()
                ),
                None,
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "{} ({}) — {} row{}{}",
                    body["path"].as_str().unwrap_or(&path),
                    body["format"].as_str().unwrap_or("dataset"),
                    body["rows"].as_array().map_or(0, Vec::len),
                    if body["rows"].as_array().map_or(0, Vec::len) == 1 {
                        ""
                    } else {
                        "s"
                    },
                    if body["truncated"].as_bool().unwrap_or(false) {
                        " (bounded)"
                    } else {
                        ""
                    }
                );
                println!("{}", body["columns"]);
                for row in body["rows"].as_array().into_iter().flatten() {
                    println!("{row}");
                }
            })
        }
        "import-hf" | "import-huggingface" => {
            if args.len() < 2 {
                bail!("usage: clotho repo import-hf <target-repo> <namespace/name> [--revision <rev>] [--path <path>]... [--max-files N] [--max-bytes N] [--allow-unsafe]");
            }
            let repo = args.remove(0);
            let source = args.remove(0);
            let revision = take_option(&mut args, "--revision").unwrap_or_else(|| "main".into());
            let paths = take_repeated(&mut args, "--path");
            let max_files = take_option(&mut args, "--max-files")
                .map(|value| {
                    value
                        .parse::<u64>()
                        .context("--max-files must be an integer")
                })
                .transpose()?
                .unwrap_or(200);
            let max_total_bytes = take_option(&mut args, "--max-bytes")
                .map(|value| {
                    value
                        .parse::<u64>()
                        .context("--max-bytes must be an integer")
                })
                .transpose()?
                .unwrap_or(10 * 1024 * 1024 * 1024);
            let allow_unsafe = take_flag(&mut args, "--allow-unsafe");
            if !args.is_empty() {
                bail!("unrecognized repo import-hf arguments: {}", args.join(" "));
            }
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/repos/{repo}/imports/huggingface"),
                Some(json!({
                    "repo_id": source,
                    "revision": revision,
                    "paths": paths,
                    "max_files": max_files,
                    "max_total_bytes": max_total_bytes,
                    "allow_unsafe": allow_unsafe,
                })),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "imported {} files / {} bytes from {}/{} into {} at {}",
                    body["files_imported"].as_u64().unwrap_or(0),
                    body["logical_bytes"].as_u64().unwrap_or(0),
                    body["source_repo_id"].as_str().unwrap_or(&source),
                    body["source_revision"].as_str().unwrap_or(&revision),
                    repo,
                    short(body["commit_id"].as_str().unwrap_or("")),
                );
                println!(
                    "Arachne files {} · security {}{}",
                    body["arachne_files"].as_u64().unwrap_or(0),
                    body["security_counts"],
                    if body["conflicted"].as_bool().unwrap_or(false) {
                        " · conflicted"
                    } else {
                        ""
                    }
                );
            })
        }
        "commit" => repo_commit(config, args).await,
        "submit" => {
            let (repo, commit_id) = require_two(&args, "clotho repo submit <repo> <commit-id>")?;
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/repos/{repo}/submit"),
                Some(json!({ "commit_id": commit_id })),
            )
            .await?;
            emit(config, &body, || print_submit(&body))
        }
        "update" => {
            let repo = require_one(&args, "clotho repo update <name>")?;
            let description = take_option(&mut args, "--description");
            let visibility = take_option(&mut args, "--visibility");
            let default_branch = take_option(&mut args, "--default-branch");
            let kind = take_option(&mut args, "--kind");
            let threshold = take_option(&mut args, "--large-file-threshold")
                .map(|value| {
                    value
                        .parse::<i64>()
                        .context("--large-file-threshold must be an integer")
                })
                .transpose()?;
            let network_mode = take_option(&mut args, "--network");
            let network_tags = take_repeated(&mut args, "--network-tag");
            if description.is_none()
                && visibility.is_none()
                && default_branch.is_none()
                && kind.is_none()
                && threshold.is_none()
                && network_mode.is_none()
                && network_tags.is_empty()
            {
                bail!("usage: clotho repo update <name> [--description] [--visibility] [--default-branch] [--kind code|model|dataset] [--large-file-threshold <bytes>] [--network public|tailscale] [--network-tag tag:name]...");
            }
            let mut patch = serde_json::Map::new();
            if let Some(d) = description {
                patch.insert("description".into(), json!(d));
            }
            if let Some(v) = visibility {
                patch.insert("visibility".into(), json!(v));
            }
            if let Some(b) = default_branch {
                patch.insert("default_branch".into(), json!(b));
            }
            if let Some(kind) = kind {
                patch.insert("kind".into(), json!(kind));
            }
            if let Some(threshold) = threshold {
                patch.insert("large_file_threshold_bytes".into(), json!(threshold));
            }
            if let Some(mode) = network_mode {
                patch.insert("network_mode".into(), json!(mode));
            }
            if !network_tags.is_empty() {
                patch.insert("network_tags".into(), json!(network_tags));
            }
            let body = request_json(
                config,
                reqwest::Method::PATCH,
                &format!("/api/v1/repos/{repo}"),
                Some(json!(patch)),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "updated {} kind={} visibility={} branch={}",
                    body["name"].as_str().unwrap_or(&repo),
                    body["kind"].as_str().unwrap_or("code"),
                    body["visibility"].as_str().unwrap_or(""),
                    body["default_branch"].as_str().unwrap_or("")
                );
            })
        }
        "delete" => {
            let repo = require_one(&args, "clotho repo delete <name>")?;
            let yes = take_flag(&mut args, "--yes");
            if !yes {
                bail!("refusing to delete {repo} without --yes");
            }
            request_value(
                config,
                reqwest::Method::DELETE,
                &format!("/api/v1/repos/{repo}"),
                None,
            )
            .await?;
            emit(config, &json!(null), || println!("deleted {repo}"))
        }
        "merge-policy" => cmd_repo_merge_policy(config, args).await,
        other => bail!("unknown repo subcommand {other:?}"),
    }
}

async fn cmd_repo_merge_policy(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!(
            "usage: clotho repo merge-policy <get|set> <repo> \
             [--require-actions] [--block-conflicted] [--approvals N] [--protect-default]"
        );
    };
    args.remove(0);
    match sub.as_str() {
        "get" => {
            let repo = require_one(&args, "clotho repo merge-policy get <repo>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/merge-policy"),
                None,
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "require_actions={} block_conflicted={} approvals={} protect_default={}",
                    body["require_passing_actions"].as_bool().unwrap_or(false),
                    body["block_merge_when_conflicted"]
                        .as_bool()
                        .unwrap_or(true),
                    body["require_review_approvals"].as_i64().unwrap_or(0),
                    body["protect_default_branch"].as_bool().unwrap_or(false),
                );
            })
        }
        "set" => {
            let repo = require_one(&args, "clotho repo merge-policy set <repo>")?;
            let require_actions = take_flag(&mut args, "--require-actions");
            let block_conflicted = take_flag(&mut args, "--block-conflicted");
            let no_block_conflicted = take_flag(&mut args, "--no-block-conflicted");
            let approvals = take_option(&mut args, "--approvals");
            let protect_default = take_flag(&mut args, "--protect-default");
            let mut patch = serde_json::Map::new();
            if require_actions {
                patch.insert("require_passing_actions".into(), json!(true));
            }
            if block_conflicted {
                patch.insert("block_merge_when_conflicted".into(), json!(true));
            }
            if no_block_conflicted {
                patch.insert("block_merge_when_conflicted".into(), json!(false));
            }
            if let Some(n) = approvals {
                let n: i32 = n.parse().context("--approvals must be an integer")?;
                patch.insert("require_review_approvals".into(), json!(n));
            }
            if protect_default {
                patch.insert("protect_default_branch".into(), json!(true));
            }
            if patch.is_empty() {
                bail!(
                    "usage: clotho repo merge-policy set <repo> \
                     [--require-actions] [--block-conflicted|--no-block-conflicted] \
                     [--approvals N] [--protect-default]"
                );
            }
            let body = request_json(
                config,
                reqwest::Method::PUT,
                &format!("/api/v1/repos/{repo}/merge-policy"),
                Some(json!(patch)),
            )
            .await?;
            emit(config, &body, || {
                println!("updated merge policy for {repo}");
            })
        }
        other => bail!("unknown merge-policy subcommand {other:?}"),
    }
}

async fn repo_commit(config: &Config, mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!(
            "usage: clotho repo commit <repo> -m <message> --file <path> \
             [--file <path> ...] [--delete <path>] [--submit]"
        );
    }
    let repo = args.remove(0);
    let message = take_option(&mut args, "-m")
        .or_else(|| take_option(&mut args, "--message"))
        .context("commit message is required: -m <message>")?;
    let submit = take_flag(&mut args, "--submit");
    let author_name = take_option(&mut args, "--author-name");
    let author_email = take_option(&mut args, "--author-email");
    let files = take_repeated(&mut args, "--file");
    let deleted = take_repeated(&mut args, "--delete");
    if !args.is_empty() {
        bail!("unrecognized commit arguments: {}", args.join(" "));
    }
    if files.is_empty() && deleted.is_empty() {
        bail!("commit requires at least one --file or --delete");
    }

    let mut file_payloads = Vec::with_capacity(files.len());
    for path in &files {
        let bytes = std::fs::read(path).with_context(|| format!("read file {}", path.display()))?;
        let mut file = json!({
            "path": repo_path(path),
            "executable": is_executable(path),
        });
        match String::from_utf8(bytes) {
            Ok(content) => file["content"] = json!(content),
            Err(err) => {
                file["content_base64"] =
                    json!(base64::engine::general_purpose::STANDARD.encode(err.into_bytes()));
            }
        }
        file_payloads.push(file);
    }

    let mut body = json!({
        "message": message,
        "files": file_payloads,
        "deleted_paths": deleted.iter().map(|p| repo_path(p)).collect::<Vec<_>>(),
    });
    if let Some(author_name) = author_name {
        body["author_name"] = json!(author_name);
    }
    if let Some(author_email) = author_email {
        body["author_email"] = json!(author_email);
    }

    let commit = request_value(
        config,
        reqwest::Method::POST,
        &format!("/api/v1/repos/{repo}/commits"),
        Some(body),
    )
    .await?;
    emit(config, &commit, || {
        println!(
            "committed {} change {} op {}",
            commit["commit_id"].as_str().unwrap_or(""),
            commit["change_id"].as_str().unwrap_or(""),
            commit["operation_id"].as_str().unwrap_or("")
        );
    })?;

    if submit {
        let commit_id = commit["commit_id"].as_str().unwrap_or("");
        let landed = request_value(
            config,
            reqwest::Method::POST,
            &format!("/api/v1/repos/{repo}/submit"),
            Some(json!({ "commit_id": commit_id })),
        )
        .await?;
        emit(config, &landed, || print_submit(&landed))?;
    }
    Ok(())
}

fn print_submit(result: &Value) {
    println!(
        "landed {} op {} fast_forwarded={} conflicted={}",
        result["commit_id"].as_str().unwrap_or(""),
        result["operation_id"].as_str().unwrap_or(""),
        result["fast_forwarded"].as_bool().unwrap_or(false),
        result["conflicted"].as_bool().unwrap_or(false)
    );
    if let Some(paths) = result["conflicted_paths"].as_array() {
        if !paths.is_empty() {
            let joined: Vec<&str> = paths.iter().filter_map(|p| p.as_str()).collect();
            println!("conflicts {}", joined.join(", "));
        }
    }
}

// ---------------------------------------------------------------------------
// issue
// ---------------------------------------------------------------------------

async fn cmd_issue(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho issue <list|create|get|comment|update> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "list" => {
            if args.is_empty() || args.len() > 2 {
                bail!("usage: clotho issue list <repo> [open|closed|all]");
            }
            let repo = &args[0];
            let state = args.get(1).map(String::as_str).unwrap_or("open");
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/issues?state={state}"),
                None,
            )
            .await?;
            emit(config, &body, || {
                for issue in body["issues"].as_array().into_iter().flatten() {
                    println!(
                        "#{} {} {}  {}",
                        issue["number"].as_i64().unwrap_or(0),
                        issue["state"].as_str().unwrap_or(""),
                        issue["title"].as_str().unwrap_or(""),
                        issue["html_url"].as_str().unwrap_or("")
                    );
                }
            })
        }
        "create" => {
            if args.is_empty() {
                bail!("usage: clotho issue create <repo> --title <title> [--body <body>] [--label <name>]... [--assignee <login>]... [--milestone <id>]");
            }
            let repo = args.remove(0);
            let title = take_option(&mut args, "--title")
                .context("issue title is required: --title <title>")?;
            let body_text = take_option(&mut args, "--body").unwrap_or_default();
            let labels = take_repeated_str(&mut args, "--label");
            let assignees = take_repeated_str(&mut args, "--assignee");
            let milestone = take_option(&mut args, "--milestone");
            if !args.is_empty() {
                bail!("unrecognized arguments: {}", args.join(" "));
            }
            let mut payload = json!({
                "title": title,
                "body": body_text,
                "labels": labels,
                "assignees": assignees,
            });
            if let Some(milestone) = milestone {
                payload["milestone"] = json!(milestone
                    .parse::<i64>()
                    .context("milestone must be an integer")?);
            }
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/repos/{repo}/issues"),
                Some(payload),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "created issue #{} {}",
                    body["number"].as_i64().unwrap_or(0),
                    body["html_url"].as_str().unwrap_or("")
                );
            })
        }
        "update" => {
            if args.len() < 2 {
                bail!("usage: clotho issue update <repo> <number> [--title <t>] [--body <b>] [--state open|closed] [--label <name>]... [--assignee <login>]... [--milestone <id>|--clear-milestone]");
            }
            let repo = args.remove(0);
            let number = args.remove(0);
            let title = take_option(&mut args, "--title");
            let body_text = take_option(&mut args, "--body");
            let state = take_option(&mut args, "--state");
            let labels = take_repeated_str(&mut args, "--label");
            let assignees = take_repeated_str(&mut args, "--assignee");
            let clear_milestone = take_flag(&mut args, "--clear-milestone");
            let milestone = take_option(&mut args, "--milestone");
            if !args.is_empty() {
                bail!("unrecognized arguments: {}", args.join(" "));
            }
            let mut payload = serde_json::Map::new();
            if let Some(title) = title {
                payload.insert("title".into(), json!(title));
            }
            if let Some(body_text) = body_text {
                payload.insert("body".into(), json!(body_text));
            }
            if let Some(state) = state {
                payload.insert("state".into(), json!(state));
            }
            if !labels.is_empty() {
                payload.insert("labels".into(), json!(labels));
            }
            if !assignees.is_empty() {
                payload.insert("assignees".into(), json!(assignees));
            }
            if clear_milestone {
                payload.insert("milestone".into(), Value::Null);
            } else if let Some(milestone) = milestone {
                payload.insert(
                    "milestone".into(),
                    json!(milestone
                        .parse::<i64>()
                        .context("milestone must be an integer")?),
                );
            }
            if payload.is_empty() {
                bail!("at least one update field is required");
            }
            let body = request_value(
                config,
                reqwest::Method::PATCH,
                &format!("/api/v1/repos/{repo}/issues/{number}"),
                Some(Value::Object(payload)),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "updated issue #{} {}",
                    body["number"].as_i64().unwrap_or(0),
                    body["html_url"].as_str().unwrap_or("")
                );
            })
        }
        "get" => {
            let (repo, number) = require_two(&args, "clotho issue get <repo> <number>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/issues/{number}"),
                None,
            )
            .await?;
            emit(config, &body, || {
                let issue = &body["issue"];
                println!(
                    "#{} {} {}",
                    issue["number"].as_i64().unwrap_or(0),
                    issue["state"].as_str().unwrap_or(""),
                    issue["title"].as_str().unwrap_or("")
                );
                if let Some(b) = issue["body"].as_str() {
                    if !b.is_empty() {
                        println!();
                        println!("{b}");
                    }
                }
                let comments = body["comments"].as_array().map_or(0, Vec::len);
                println!("\n{comments} comment(s)");
            })
        }
        "comment" => {
            if args.len() < 2 {
                bail!("usage: clotho issue comment <repo> <number> --body <text>");
            }
            let repo = args.remove(0);
            let number = args.remove(0);
            let text =
                take_option(&mut args, "--body").context("comment body required: --body <text>")?;
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/repos/{repo}/issues/{number}/comments"),
                Some(json!({ "body": text })),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "commented {} {}",
                    body["id"].as_i64().unwrap_or(0),
                    body["html_url"].as_str().unwrap_or("")
                );
            })
        }
        other => bail!("unknown issue subcommand {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// label
// ---------------------------------------------------------------------------

async fn cmd_label(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho label <list|create> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "list" => {
            let repo = require_one(&args, "clotho label list <repo>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/labels"),
                None,
            )
            .await?;
            emit(config, &body, || {
                for label in body["labels"].as_array().into_iter().flatten() {
                    println!(
                        "{} {} {}",
                        label["name"].as_str().unwrap_or("?"),
                        label["color"].as_str().unwrap_or(""),
                        label["description"].as_str().unwrap_or("")
                    );
                }
            })
        }
        "create" => {
            if args.is_empty() {
                bail!("usage: clotho label create <repo> --name <name> --color <hex> [--description <text>]");
            }
            let repo = args.remove(0);
            let name = take_option(&mut args, "--name")
                .context("label name is required: --name <name>")?;
            let color = take_option(&mut args, "--color")
                .context("label color is required: --color <hex>")?;
            let description = take_option(&mut args, "--description").unwrap_or_default();
            if !args.is_empty() {
                bail!("unrecognized arguments: {}", args.join(" "));
            }
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/repos/{repo}/labels"),
                Some(json!({ "name": name, "color": color, "description": description })),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "created label {} ({})",
                    body["name"].as_str().unwrap_or("?"),
                    body["color"].as_str().unwrap_or("")
                );
            })
        }
        other => bail!("unknown label subcommand {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// milestone
// ---------------------------------------------------------------------------

async fn cmd_milestone(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho milestone <list|create> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "list" => {
            let repo = require_one(&args, "clotho milestone list <repo>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/milestones"),
                None,
            )
            .await?;
            emit(config, &body, || {
                for ms in body["milestones"].as_array().into_iter().flatten() {
                    println!(
                        "#{} {} {} {}",
                        ms["id"].as_i64().unwrap_or(0),
                        ms["state"].as_str().unwrap_or(""),
                        ms["title"].as_str().unwrap_or("?"),
                        ms["due_on"].as_str().unwrap_or("")
                    );
                }
            })
        }
        "create" => {
            if args.is_empty() {
                bail!("usage: clotho milestone create <repo> --title <title> [--description <text>] [--due-on <iso8601>]");
            }
            let repo = args.remove(0);
            let title = take_option(&mut args, "--title")
                .context("milestone title is required: --title <title>")?;
            let description = take_option(&mut args, "--description").unwrap_or_default();
            let due_on = take_option(&mut args, "--due-on");
            if !args.is_empty() {
                bail!("unrecognized arguments: {}", args.join(" "));
            }
            let mut payload = json!({ "title": title, "description": description });
            if let Some(due_on) = due_on {
                payload["due_on"] = json!(due_on);
            }
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/repos/{repo}/milestones"),
                Some(payload),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "created milestone #{} {}",
                    body["id"].as_i64().unwrap_or(0),
                    body["title"].as_str().unwrap_or("?")
                );
            })
        }
        other => bail!("unknown milestone subcommand {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// notification
// ---------------------------------------------------------------------------

async fn cmd_notification(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho notification <list|read> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "list" => {
            let unread = take_flag(&mut args, "--unread");
            if !args.is_empty() {
                bail!("usage: clotho notification list [--unread]");
            }
            let mut path = "/api/v1/notifications".to_string();
            if unread {
                path.push_str("?unread=true");
            }
            let body: Value = request_json(config, reqwest::Method::GET, &path, None).await?;
            emit(config, &body, || {
                println!("unread: {}", body["unread_count"].as_i64().unwrap_or(0));
                for n in body["notifications"].as_array().into_iter().flatten() {
                    println!(
                        "[{}] {} — {}",
                        n["kind"].as_str().unwrap_or("?"),
                        n["title"].as_str().unwrap_or(""),
                        n["href"].as_str().unwrap_or("")
                    );
                }
            })
        }
        "read" => {
            let all = take_flag(&mut args, "--all");
            if !args.is_empty() {
                bail!("usage: clotho notification read [--all]");
            }
            let body = request_value(
                config,
                reqwest::Method::POST,
                "/api/v1/notifications/mark-read",
                Some(json!({ "all": all })),
            )
            .await?;
            emit(config, &body, || println!("marked read"))
        }
        other => bail!("unknown notification subcommand {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// pr
// ---------------------------------------------------------------------------

async fn cmd_pr(config: &Config, mut args: Vec<String>) -> Result<()> {
    // Stage 8 compat: `clotho pr <repo> [state]` → list
    if !args.is_empty() && !is_pr_sub(&args[0]) {
        args.insert(0, "list".into());
    }
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho pr <list|create|get|comment|review|merge|diff> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "list" => {
            if args.is_empty() || args.len() > 2 {
                bail!("usage: clotho pr list <repo> [open|closed|all]");
            }
            let repo = &args[0];
            let state = args.get(1).map(String::as_str).unwrap_or("open");
            if !matches!(state, "open" | "closed" | "all") {
                bail!("pr state must be open, closed, or all");
            }
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/pulls?state={state}"),
                None,
            )
            .await?;
            emit(config, &body, || {
                for pull in body["pulls"].as_array().into_iter().flatten() {
                    println!(
                        "#{} {} {} {}",
                        pull["number"].as_i64().unwrap_or(0),
                        pull["state"].as_str().unwrap_or(""),
                        pull["title"].as_str().unwrap_or(""),
                        pull["html_url"].as_str().unwrap_or("")
                    );
                }
            })
        }
        "create" => {
            if args.is_empty() {
                bail!(
                    "usage: clotho pr create <repo> --title <title> --head <branch> \
                     [--base <branch>] [--body <body>]"
                );
            }
            let repo = args.remove(0);
            let title =
                take_option(&mut args, "--title").context("PR title required: --title <title>")?;
            let head =
                take_option(&mut args, "--head").context("PR head required: --head <branch>")?;
            let base = take_option(&mut args, "--base").unwrap_or_else(|| "main".into());
            let body_text = take_option(&mut args, "--body").unwrap_or_default();
            if !args.is_empty() {
                bail!("unrecognized arguments: {}", args.join(" "));
            }
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/repos/{repo}/pulls"),
                Some(json!({
                    "title": title,
                    "head": head,
                    "base": base,
                    "body": body_text,
                })),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "created PR #{} {}",
                    body["number"].as_i64().unwrap_or(0),
                    body["html_url"].as_str().unwrap_or("")
                );
            })
        }
        "get" => {
            let (repo, number) = require_two(&args, "clotho pr get <repo> <number>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/pulls/{number}"),
                None,
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "#{} {} {} → {}",
                    body["number"].as_i64().unwrap_or(0),
                    body["state"].as_str().unwrap_or(""),
                    body["head"]["ref"].as_str().unwrap_or("?"),
                    body["base"]["ref"].as_str().unwrap_or("?")
                );
                println!("{}", body["title"].as_str().unwrap_or(""));
                println!(
                    "mergeable={} merged={}",
                    body["mergeable"].as_bool().unwrap_or(false),
                    body["merged"].as_bool().unwrap_or(false)
                );
            })
        }
        "comment" => {
            if args.len() < 2 {
                bail!("usage: clotho pr comment <repo> <number> --body <text>");
            }
            let repo = args.remove(0);
            let number = args.remove(0);
            let text =
                take_option(&mut args, "--body").context("comment body required: --body <text>")?;
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/repos/{repo}/pulls/{number}/comments"),
                Some(json!({ "body": text })),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "commented {} {}",
                    body["id"].as_i64().unwrap_or(0),
                    body["html_url"].as_str().unwrap_or("")
                );
            })
        }
        "review" => {
            if args.len() < 2 {
                bail!(
                    "usage: clotho pr review <repo> <number> \
                     --event COMMENT|APPROVE|REQUEST_CHANGES [--body <text>]"
                );
            }
            let repo = args.remove(0);
            let number = args.remove(0);
            let event = take_option(&mut args, "--event").unwrap_or_else(|| "COMMENT".into());
            let body_text = take_option(&mut args, "--body").unwrap_or_default();
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/repos/{repo}/pulls/{number}/reviews"),
                Some(json!({ "event": event, "body": body_text })),
            )
            .await?;
            emit(config, &body, || {
                println!("reviewed PR #{number}");
            })
        }
        "merge" => {
            if args.len() < 2 {
                bail!(
                    "usage: clotho pr merge <repo> <number> \
                     [--method merge|rebase|rebase-merge|squash]"
                );
            }
            let repo = args.remove(0);
            let number = args.remove(0);
            let method = take_option(&mut args, "--method").unwrap_or_else(|| "merge".into());
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/repos/{repo}/pulls/{number}/merge"),
                Some(json!({ "method": method })),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "merged PR #{} {}",
                    body["number"].as_i64().unwrap_or(0),
                    body["state"].as_str().unwrap_or("")
                );
            })
        }
        "diff" => {
            let (repo, number) = require_two(&args, "clotho pr diff <repo> <number>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/pulls/{number}/diff"),
                None,
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "{}..{} conflicted={}",
                    short(body["from_commit_id"].as_str().unwrap_or("")),
                    short(body["to_commit_id"].as_str().unwrap_or("")),
                    body["conflicted"].as_bool().unwrap_or(false)
                );
                for f in body["files"].as_array().into_iter().flatten() {
                    println!(
                        "  {} {}",
                        f["status"].as_str().unwrap_or("?"),
                        f["path"].as_str().unwrap_or("?")
                    );
                }
            })
        }
        other => bail!("unknown pr subcommand {other:?}"),
    }
}

fn is_pr_sub(s: &str) -> bool {
    matches!(
        s,
        "list" | "create" | "get" | "comment" | "review" | "merge" | "diff"
    )
}

// ---------------------------------------------------------------------------
// actions
// ---------------------------------------------------------------------------

async fn cmd_actions(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho actions <list|run|get|logs|config> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "list" => {
            let repo = require_one(&args, "clotho actions list <repo>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/actions/runs"),
                None,
            )
            .await?;
            emit(config, &body, || {
                for run in body["runs"].as_array().into_iter().flatten() {
                    println!(
                        "{}  {}  {}  {}  {}",
                        run["id"].as_str().unwrap_or("?"),
                        run["status"].as_str().unwrap_or(""),
                        run["conclusion"].as_str().unwrap_or("-"),
                        short(run["commit_id"].as_str().unwrap_or("")),
                        run["provider"].as_str().unwrap_or("")
                    );
                }
            })
        }
        "run" | "start" => {
            if args.is_empty() {
                bail!(
                    "usage: clotho actions run <repo> [--commit <id>] [--branch <name>] \
                     [--actor <name>]"
                );
            }
            let repo = args.remove(0);
            let commit_id = take_option(&mut args, "--commit").unwrap_or_default();
            let branch = take_option(&mut args, "--branch").unwrap_or_else(|| "main".into());
            let actor = take_option(&mut args, "--actor").unwrap_or_else(|| "cli".into());
            if !args.is_empty() {
                bail!("unrecognized arguments: {}", args.join(" "));
            }
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/repos/{repo}/actions/runs"),
                Some(json!({
                    "commit_id": commit_id,
                    "branch": branch,
                    "actor": actor,
                })),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "started run {} status={}",
                    body["id"].as_str().unwrap_or("?"),
                    body["status"].as_str().unwrap_or("")
                );
            })
        }
        "get" => {
            let (repo, run_id) = require_two(&args, "clotho actions get <repo> <run-id>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/actions/runs/{run_id}"),
                None,
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "{}  {}  conclusion={}  provider={}  commit={}",
                    body["id"].as_str().unwrap_or("?"),
                    body["status"].as_str().unwrap_or(""),
                    body["conclusion"].as_str().unwrap_or("-"),
                    body["provider"].as_str().unwrap_or(""),
                    short(body["commit_id"].as_str().unwrap_or(""))
                );
            })
        }
        "logs" => {
            let (repo, run_id) = require_two(&args, "clotho actions logs <repo> <run-id>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/actions/runs/{run_id}/logs"),
                None,
            )
            .await?;
            emit(config, &body, || {
                print!("{}", body["text"].as_str().unwrap_or(""));
                if !body["text"].as_str().unwrap_or("").ends_with('\n') {
                    println!();
                }
            })
        }
        "config" => {
            if args.is_empty() {
                bail!(
                    "usage: clotho actions config <repo> [--provider <id>] [--enabled true|false]"
                );
            }
            let repo = args.remove(0);
            let provider = take_option(&mut args, "--provider");
            let enabled = take_option(&mut args, "--enabled");
            let accelerator = take_option(&mut args, "--accelerator");
            let gpu_types = take_repeated(&mut args, "--gpu-type");
            if provider.is_none()
                && enabled.is_none()
                && accelerator.is_none()
                && gpu_types.is_empty()
            {
                let body: Value = request_json(
                    config,
                    reqwest::Method::GET,
                    &format!("/api/v1/repos/{repo}/actions/config"),
                    None,
                )
                .await?;
                return emit(config, &body, || {
                    println!(
                        "enabled={} provider={} accelerator={} image={} timeout={}s",
                        body["enabled"].as_bool().unwrap_or(false),
                        body["provider"].as_str().unwrap_or(""),
                        body["accelerator"].as_str().unwrap_or("cpu"),
                        body["default_image"].as_str().unwrap_or(""),
                        body["timeout_seconds"].as_u64().unwrap_or(0)
                    );
                });
            }
            // Fetch current then PATCH via PUT full body.
            let mut cfg: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/repos/{repo}/actions/config"),
                None,
            )
            .await?;
            if let Some(p) = provider {
                cfg["provider"] = json!(p);
            }
            if let Some(e) = enabled {
                cfg["enabled"] = json!(e == "true" || e == "1" || e == "yes");
            }
            if let Some(accelerator) = accelerator {
                cfg["accelerator"] = json!(accelerator);
            }
            if !gpu_types.is_empty() {
                cfg["gpu_types"] = json!(gpu_types);
            }
            let body = request_value(
                config,
                reqwest::Method::PUT,
                &format!("/api/v1/repos/{repo}/actions/config"),
                Some(cfg),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "updated enabled={} provider={} accelerator={}",
                    body["enabled"].as_bool().unwrap_or(false),
                    body["provider"].as_str().unwrap_or(""),
                    body["accelerator"].as_str().unwrap_or("cpu")
                );
            })
        }
        other => bail!("unknown actions subcommand {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// provider
// ---------------------------------------------------------------------------

async fn cmd_provider(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho provider <list|get|connect|disconnect> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "list" => {
            let layer = take_option(&mut args, "--layer");
            let all = take_flag(&mut args, "--all");
            let org = take_option(&mut args, "--org");
            let org_query = org.map(|org| format!("&org={org}")).unwrap_or_default();
            let path = if let Some(layer) = layer {
                format!("/api/v1/providers?layer={layer}{org_query}")
            } else if all {
                format!("/api/v1/providers?all=true{org_query}")
            } else {
                "/api/v1/providers".to_string()
            };
            let body: Value = request_json(config, reqwest::Method::GET, &path, None).await?;
            emit(config, &body, || {
                println!(
                    "default {}",
                    body["default_provider_id"].as_str().unwrap_or("")
                );
                if let Some(layer) = body["layer"].as_str() {
                    println!("layer {layer}");
                }
                for p in body["providers"].as_array().into_iter().flatten() {
                    let configured = if p["configured"].as_bool().unwrap_or(false) {
                        "configured"
                    } else {
                        "not-configured"
                    };
                    let reason = p["configured_reason"].as_str().unwrap_or("");
                    let layer = p["layer"].as_str().unwrap_or("");
                    println!(
                        "{}{}  {}  {}  {}{}",
                        p["id"].as_str().unwrap_or("?"),
                        if layer.is_empty() {
                            String::new()
                        } else {
                            format!("[{layer}]")
                        },
                        p["kind"].as_str().unwrap_or(""),
                        if p["enabled"].as_bool().unwrap_or(false) {
                            "enabled"
                        } else {
                            "disabled"
                        },
                        configured,
                        if reason.is_empty() {
                            String::new()
                        } else {
                            format!("  ({reason})")
                        }
                    );
                }
            })
        }
        "get" => {
            let id = require_one(&args, "clotho provider get <id>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/providers/{id}"),
                None,
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "{}  configured={}  {}",
                    body["id"].as_str().unwrap_or("?"),
                    body["configured"].as_bool().unwrap_or(false),
                    body["configured_reason"].as_str().unwrap_or("")
                );
            })
        }
        "connect" => {
            if args.is_empty() {
                bail!("usage: clotho provider connect <id> (--api-key <key> | --client-id <id> --client-secret <secret>) [--org <org>]");
            }
            let id = args.remove(0);
            let api_key = take_option(&mut args, "--api-key").unwrap_or_default();
            let client_id = take_option(&mut args, "--client-id").unwrap_or_default();
            let client_secret = take_option(&mut args, "--client-secret").unwrap_or_default();
            let org = take_option(&mut args, "--org").unwrap_or_default();
            if api_key.is_empty() && (client_id.is_empty() || client_secret.is_empty()) {
                bail!("provider credentials required: --api-key, or both --client-id and --client-secret");
            }
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/providers/{id}/connect"),
                Some(json!({
                    "api_key": api_key,
                    "client_id": client_id,
                    "client_secret": client_secret,
                    "org": org,
                })),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "connected secret {} …{}",
                    body["name"].as_str().unwrap_or("?"),
                    body["value_last4"].as_str().unwrap_or("")
                );
            })
        }
        "disconnect" => {
            if args.is_empty() {
                bail!("usage: clotho provider disconnect <id> [--org <org>]");
            }
            let id = args.remove(0);
            let org = take_option(&mut args, "--org").unwrap_or_default();
            let body = request_value(
                config,
                reqwest::Method::DELETE,
                &format!("/api/v1/providers/{id}/connect?org={org}"),
                None,
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "disconnected {} ({} secrets removed)",
                    body["provider"].as_str().unwrap_or(&id),
                    body["deleted_secrets"].as_array().map_or(0, Vec::len)
                );
            })
        }
        other => bail!("unknown provider subcommand {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// secret
// ---------------------------------------------------------------------------

async fn cmd_secret(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho secret <list|set|get|delete> <org|repo> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "list" => {
            let (scope, name) = require_two(&args, "clotho secret list <org|repo> <name>")?;
            let path = secret_list_path(&scope, &name)?;
            let body: Value = request_json(config, reqwest::Method::GET, &path, None).await?;
            emit(config, &body, || {
                for s in body["secrets"].as_array().into_iter().flatten() {
                    println!(
                        "{}  …{}  {}",
                        s["name"].as_str().unwrap_or("?"),
                        s["value_last4"].as_str().unwrap_or(""),
                        s["description"].as_str().unwrap_or("")
                    );
                }
            })
        }
        "set" => {
            if args.len() < 2 {
                bail!(
                    "usage: clotho secret set <org|repo> <name> --name <secret> --value <val> \
                     [--description <d>]"
                );
            }
            let scope = args.remove(0);
            let owner = args.remove(0);
            let secret_name =
                take_option(&mut args, "--name").context("secret name required: --name <name>")?;
            let value = take_option(&mut args, "--value")
                .context("secret value required: --value <val>")?;
            let description = take_option(&mut args, "--description").unwrap_or_default();
            let path = secret_list_path(&scope, &owner)?;
            let body = request_value(
                config,
                reqwest::Method::POST,
                &path,
                Some(json!({
                    "name": secret_name,
                    "value": value,
                    "description": description,
                })),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "stored {} …{} (value not returned)",
                    body["name"].as_str().unwrap_or(&secret_name),
                    body["value_last4"].as_str().unwrap_or("")
                );
            })
        }
        "get" => {
            if args.len() != 3 {
                bail!("usage: clotho secret get <org|repo> <owner> <secret-name>");
            }
            let path = secret_item_path(&args[0], &args[1], &args[2])?;
            let body: Value = request_json(config, reqwest::Method::GET, &path, None).await?;
            emit(config, &body, || {
                println!(
                    "{}  …{}  {}",
                    body["name"].as_str().unwrap_or("?"),
                    body["value_last4"].as_str().unwrap_or(""),
                    body["description"].as_str().unwrap_or("")
                );
            })
        }
        "delete" => {
            if args.len() != 3 {
                bail!("usage: clotho secret delete <org|repo> <owner> <secret-name>");
            }
            let path = secret_item_path(&args[0], &args[1], &args[2])?;
            let body = request_value(config, reqwest::Method::DELETE, &path, None).await?;
            emit(config, &json!({ "deleted": true, "path": path }), || {
                println!("deleted {}", args[2]);
                let _ = body;
            })
        }
        other => bail!("unknown secret subcommand {other:?}"),
    }
}

fn secret_list_path(scope: &str, name: &str) -> Result<String> {
    match scope {
        "org" => Ok(format!("/api/v1/orgs/{name}/secrets")),
        "repo" => Ok(format!("/api/v1/repos/{name}/secrets")),
        other => bail!("scope must be org or repo, got {other:?}"),
    }
}

fn secret_item_path(scope: &str, owner: &str, secret: &str) -> Result<String> {
    match scope {
        "org" => Ok(format!("/api/v1/orgs/{owner}/secrets/{secret}")),
        "repo" => Ok(format!("/api/v1/repos/{owner}/secrets/{secret}")),
        other => bail!("scope must be org or repo, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// agent identity admin (Slice C)
// ---------------------------------------------------------------------------

fn parse_scope_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

async fn cmd_agent(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho agent <list|create|tokens|mint|revoke|audit> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "list" => {
            let body: Value =
                request_json(config, reqwest::Method::GET, "/api/v1/agents", None).await?;
            emit(config, &body, || {
                for agent in body["agents"].as_array().into_iter().flatten() {
                    println!(
                        "{}  {}",
                        agent["name"].as_str().unwrap_or("?"),
                        agent["description"].as_str().unwrap_or("")
                    );
                }
            })
        }
        "create" => {
            if args.is_empty() {
                bail!("usage: clotho agent create <name> [--description <text>]");
            }
            let name = args.remove(0);
            let description = take_option(&mut args, "--description").unwrap_or_default();
            let body = request_json(
                config,
                reqwest::Method::POST,
                "/api/v1/agents",
                Some(json!({ "name": name, "description": description })),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "created agent {} ({})",
                    body["name"].as_str().unwrap_or("?"),
                    body["id"].as_str().unwrap_or("")
                );
            })
        }
        "tokens" => {
            let name = require_one(&args, "clotho agent tokens <name>")?;
            let path = format!("/api/v1/agents/{name}/tokens");
            let body: Value = request_json(config, reqwest::Method::GET, &path, None).await?;
            emit(config, &body, || {
                for token in body["tokens"].as_array().into_iter().flatten() {
                    let revoked = token["revoked_at"].as_str().is_some();
                    println!(
                        "{}  {}  repos={}  tools={}{}",
                        token["id"].as_str().unwrap_or("?"),
                        token["token_prefix"].as_str().unwrap_or(""),
                        serde_json::to_string(&token["allowed_repos"]).unwrap_or_default(),
                        serde_json::to_string(&token["allowed_tools"]).unwrap_or_default(),
                        if revoked { "  (revoked)" } else { "" }
                    );
                }
            })
        }
        "mint" => {
            if args.is_empty() {
                bail!("usage: clotho agent mint <name> --repos <list> --tools <list>");
            }
            let name = args.remove(0);
            let repos_raw = take_option(&mut args, "--repos")
                .ok_or_else(|| anyhow::anyhow!("--repos is required"))?;
            let tools_raw = take_option(&mut args, "--tools")
                .ok_or_else(|| anyhow::anyhow!("--tools is required"))?;
            let expires_secs = take_option(&mut args, "--expires-secs")
                .map(|s| s.parse::<i64>())
                .transpose()?;
            let path = format!("/api/v1/agents/{name}/tokens");
            let mut payload = json!({
                "allowed_repos": parse_scope_list(&repos_raw),
                "allowed_tools": parse_scope_list(&tools_raw),
            });
            if let Some(secs) = expires_secs {
                payload["expires_in_secs"] = json!(secs);
            }
            let body = request_json(config, reqwest::Method::POST, &path, Some(payload)).await?;
            emit(config, &body, || {
                println!(
                    "token {} for agent {} — save this value, it is shown once:\n{}",
                    body["token_id"].as_str().unwrap_or("?"),
                    body["agent"].as_str().unwrap_or(&name),
                    body["token"].as_str().unwrap_or("")
                );
            })
        }
        "revoke" => {
            let (name, token_id) = require_two(&args, "clotho agent revoke <name> <token_id>")?;
            let path = format!("/api/v1/agents/{name}/tokens/{token_id}");
            let _ = request_value(config, reqwest::Method::DELETE, &path, None).await?;
            emit(
                config,
                &json!({ "revoked": token_id, "agent": name }),
                || {
                    println!("revoked token {token_id} for agent {name}");
                },
            )
        }
        "audit" => {
            if args.is_empty() {
                bail!("usage: clotho agent audit <name> [--limit N]");
            }
            let name = args.remove(0);
            let limit = take_option(&mut args, "--limit")
                .map(|s| s.parse::<i64>())
                .transpose()?
                .unwrap_or(50);
            let path = format!("/api/v1/agents/{name}/audit?limit={limit}");
            let body: Value = request_json(config, reqwest::Method::GET, &path, None).await?;
            emit(config, &body, || {
                for entry in body["entries"].as_array().into_iter().flatten() {
                    println!(
                        "{}  {}  {}  {}  {}",
                        entry["occurred_at"].as_str().unwrap_or(""),
                        entry["tool"].as_str().unwrap_or("?"),
                        entry["repo"].as_str().unwrap_or(""),
                        entry["status"].as_str().unwrap_or("?"),
                        entry["token_id"].as_str().unwrap_or("")
                    );
                }
            })
        }
        other => bail!("unknown agent subcommand {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// org + activity
// ---------------------------------------------------------------------------

async fn cmd_org(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho org <list|create|get|repos> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "list" => {
            let body: Value =
                request_json(config, reqwest::Method::GET, "/api/v1/orgs", None).await?;
            emit(config, &body, || {
                for org in body["orgs"].as_array().into_iter().flatten() {
                    println!(
                        "{}  {}",
                        org["name"].as_str().unwrap_or("?"),
                        org["display_name"].as_str().unwrap_or("")
                    );
                }
            })
        }
        "create" => {
            if args.is_empty() {
                bail!("usage: clotho org create <name> [--display-name <d>]");
            }
            let name = args.remove(0);
            let display_name = take_option(&mut args, "--display-name");
            let mut payload = json!({ "name": name });
            if let Some(d) = display_name {
                payload["display_name"] = json!(d);
            }
            let body =
                request_value(config, reqwest::Method::POST, "/api/v1/orgs", Some(payload)).await?;
            emit(config, &body, || {
                println!("created org {}", body["name"].as_str().unwrap_or(&name));
            })
        }
        "get" => {
            let name = require_one(&args, "clotho org get <name>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/orgs/{name}"),
                None,
            )
            .await?;
            emit(config, &body, || {
                let org = &body["org"];
                println!(
                    "{}  {}",
                    org["name"].as_str().unwrap_or("?"),
                    org["display_name"].as_str().unwrap_or("")
                );
                let members = body["members"].as_array().map_or(0, Vec::len);
                println!("{members} member(s)");
            })
        }
        "repos" => {
            let name = require_one(&args, "clotho org repos <name>")?;
            let body: Value = request_json(
                config,
                reqwest::Method::GET,
                &format!("/api/v1/orgs/{name}/repos"),
                None,
            )
            .await?;
            emit(config, &body, || {
                for repo in body["repos"].as_array().into_iter().flatten() {
                    println!("{}", repo["name"].as_str().unwrap_or("?"));
                }
            })
        }
        other => bail!("unknown org subcommand {other:?}"),
    }
}

async fn cmd_activity(config: &Config, mut args: Vec<String>) -> Result<()> {
    let limit = take_option(&mut args, "--limit").unwrap_or_else(|| "20".into());
    if !args.is_empty() {
        bail!("usage: clotho activity [--limit N]");
    }
    let body: Value = request_json(
        config,
        reqwest::Method::GET,
        &format!("/api/v1/activity?limit={limit}"),
        None,
    )
    .await?;
    emit(config, &body, || {
        for ev in body["events"].as_array().into_iter().flatten() {
            println!(
                "{}  {}  actor={}",
                ev["created_at"].as_str().unwrap_or(""),
                ev["event_type"].as_str().unwrap_or("?"),
                ev["actor_id"].as_str().unwrap_or("")
            );
        }
    })
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn repo_path(path: &Path) -> String {
    path.to_string_lossy().trim_start_matches("./").to_string()
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    false
}

fn usage() {
    eprintln!(
        "usage:
  clotho [--api <url>] [--token <tok>] [--json] <command> ...

  Global:
    --api <url>     API gateway (default $CLOTHO_API_URL or http://localhost:8080)
    --token <tok>   Bearer token (default $CLOTHO_TOKEN)
    --json          Machine-readable JSON on stdout

  auth
    clotho auth whoami
    clotho auth token create [--name <label>]
    clotho auth token list
    clotho auth token revoke <id>

  repo
    clotho repo init <name> [--kind code|model|dataset] [--large-file-threshold <bytes>] [--network public|tailscale] [--network-tag tag:name]...
    clotho repo list
    clotho repo status <repo>
    clotho repo update <name> [--description] [--visibility] [--default-branch] [--kind] [--large-file-threshold] [--network] [--network-tag]...
    clotho repo merge-policy get <repo>
    clotho repo merge-policy set <repo> [--require-actions] [--block-conflicted|--no-block-conflicted] [--approvals N] [--protect-default]
    clotho repo delete <name> [--yes]
    clotho repo log <repo>
    clotho repo tree <repo>
    clotho repo artifacts <repo>
    clotho repo preview <repo> <csv|tsv|jsonl-path> [--limit 1..100]
    clotho repo import-hf <target> <namespace/name> [--revision <rev>] [--path <path>]... [--max-files N] [--max-bytes N]
    clotho repo commit <repo> -m <msg> --file <path> [...] [--submit]
    clotho repo submit <repo> <commit-id>

  issue
    clotho issue list <repo> [open|closed|all]
    clotho issue create <repo> --title <t> [--body <b>] [--label <name>]... [--assignee <login>]... [--milestone <id>]
    clotho issue update <repo> <n> [--title <t>] [--body <b>] [--state open|closed] [--label <name>]... [--assignee <login>]... [--milestone <id>|--clear-milestone]
    clotho issue get <repo> <number>
    clotho issue comment <repo> <number> --body <text>

  label
    clotho label list <repo>
    clotho label create <repo> --name <name> --color <hex> [--description <text>]

  milestone
    clotho milestone list <repo>
    clotho milestone create <repo> --title <title> [--description <text>] [--due-on <iso8601>]

  notification
    clotho notification list [--unread]
    clotho notification read [--all]

  pr
    clotho pr list <repo> [open|closed|all]
    clotho pr create <repo> --title <t> --head <branch> [--base main] [--body <b>]
    clotho pr get <repo> <number>
    clotho pr comment <repo> <number> --body <text>
    clotho pr review <repo> <number> --event COMMENT|APPROVE|REQUEST_CHANGES
    clotho pr merge <repo> <number> [--method merge|squash|rebase]
    clotho pr diff <repo> <number>

  actions
    clotho actions list <repo>
    clotho actions run <repo> [--commit <id>] [--branch main] [--actor cli]
    clotho actions get <repo> <run-id>
    clotho actions logs <repo> <run-id>
    clotho actions config <repo> [--provider <id>] [--enabled true|false] [--accelerator cpu|gpu] [--gpu-type <id>]...

  provider
    clotho provider list [--layer compute|storage|network|hub|auth] [--all]
    clotho provider get <id>
    clotho provider connect <id> (--api-key <key> | --client-id <id> --client-secret <secret>) [--org <org>]
    clotho provider disconnect <id> [--org <org>]

  secret   (values are write-only; responses are metadata + last4)
    clotho secret list org|repo <name>
    clotho secret set org|repo <owner> --name <n> --value <v>
    clotho secret get org|repo <owner> <secret-name>
    clotho secret delete org|repo <owner> <secret-name>

  org / activity
    clotho org list|create|get|repos ...
    clotho activity [--limit N]

  agent   (requires org admin or bootstrap; CLOTHO_AGENT_ADMIN_TOKEN on gateway)
    clotho agent list
    clotho agent create <name> [--description <text>]
    clotho agent tokens <name>
    clotho agent mint <name> --repos <a,b|*> --tools <a,b|*> [--expires-secs N]
    clotho agent revoke <name> <token_id>
    clotho agent audit <name> [--limit N]

  Stage 8 aliases (still work): init, status, log, commit, submit, pr <repo>"
    );
}
