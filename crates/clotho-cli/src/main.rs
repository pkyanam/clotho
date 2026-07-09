//! `clotho` CLI: thin human-facing client over the api-gateway REST edge.
//! Never shells out to git or jj; local files are read and sent to the gateway.

mod args;
mod client;

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::args::{require_one, require_two, strip_globals, take_flag, take_option, take_repeated};
use crate::client::{emit, first_line, request_json, request_value, short, Config};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let (api_opt, json) = strip_globals(&mut args);
    let api_url = api_opt
        .or_else(|| std::env::var("CLOTHO_API_URL").ok())
        .unwrap_or_else(|| "http://localhost:8080".into());
    let config = Config::from_env_and_args(api_url, json);

    let Some(command) = args.first().cloned() else {
        usage();
        return Ok(());
    };
    args.remove(0);

    match command.as_str() {
        // Grouped commands (Stage 15).
        "repo" => cmd_repo(&config, args).await,
        "issue" => cmd_issue(&config, args).await,
        "pr" => cmd_pr(&config, args).await,
        "actions" => cmd_actions(&config, args).await,
        "provider" => cmd_provider(&config, args).await,
        "secret" => cmd_secret(&config, args).await,
        "org" => cmd_org(&config, args).await,
        "activity" => cmd_activity(&config, args).await,
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
// repo
// ---------------------------------------------------------------------------

async fn cmd_repo(config: &Config, mut args: Vec<String>) -> Result<()> {
    let Some(sub) = args.first().cloned() else {
        bail!("usage: clotho repo <init|list|status|log|commit|submit|tree|get> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "init" | "create" => {
            let name = require_one(&args, "clotho repo init <name>")?;
            let body = request_value(
                config,
                reqwest::Method::POST,
                "/api/v1/repos",
                Some(json!({ "name": name })),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "created {}/{} at {}",
                    body["owner"].as_str().unwrap_or("?"),
                    body["name"].as_str().unwrap_or(&name),
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
                println!(
                    "heads {}",
                    detail["heads"].as_array().map_or(0, Vec::len)
                );
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
        other => bail!("unknown repo subcommand {other:?}"),
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
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("read text file {}", path.display()))?;
        file_payloads.push(json!({
            "path": repo_path(path),
            "content": content,
            "executable": is_executable(path),
        }));
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
        bail!("usage: clotho issue <list|create|get|comment> ...");
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
                bail!("usage: clotho issue create <repo> --title <title> [--body <body>]");
            }
            let repo = args.remove(0);
            let title = take_option(&mut args, "--title")
                .context("issue title is required: --title <title>")?;
            let body_text = take_option(&mut args, "--body").unwrap_or_default();
            if !args.is_empty() {
                bail!("unrecognized arguments: {}", args.join(" "));
            }
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/repos/{repo}/issues"),
                Some(json!({ "title": title, "body": body_text })),
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
            let event =
                take_option(&mut args, "--event").unwrap_or_else(|| "COMMENT".into());
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
                if !body["text"]
                    .as_str()
                    .unwrap_or("")
                    .ends_with('\n')
                {
                    println!();
                }
            })
        }
        "config" => {
            if args.is_empty() {
                bail!("usage: clotho actions config <repo> [--provider <id>] [--enabled true|false]");
            }
            let repo = args.remove(0);
            let provider = take_option(&mut args, "--provider");
            let enabled = take_option(&mut args, "--enabled");
            if provider.is_none() && enabled.is_none() {
                let body: Value = request_json(
                    config,
                    reqwest::Method::GET,
                    &format!("/api/v1/repos/{repo}/actions/config"),
                    None,
                )
                .await?;
                return emit(config, &body, || {
                    println!(
                        "enabled={} provider={} image={} timeout={}s",
                        body["enabled"].as_bool().unwrap_or(false),
                        body["provider"].as_str().unwrap_or(""),
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
            let body = request_value(
                config,
                reqwest::Method::PUT,
                &format!("/api/v1/repos/{repo}/actions/config"),
                Some(cfg),
            )
            .await?;
            emit(config, &body, || {
                println!(
                    "updated enabled={} provider={}",
                    body["enabled"].as_bool().unwrap_or(false),
                    body["provider"].as_str().unwrap_or("")
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
        bail!("usage: clotho provider <list|get|connect> ...");
    };
    args.remove(0);
    match sub.as_str() {
        "list" => {
            let body: Value =
                request_json(config, reqwest::Method::GET, "/api/v1/providers", None).await?;
            emit(config, &body, || {
                println!(
                    "default {}",
                    body["default_provider_id"].as_str().unwrap_or("")
                );
                for p in body["providers"].as_array().into_iter().flatten() {
                    let configured = if p["configured"].as_bool().unwrap_or(false) {
                        "configured"
                    } else {
                        "not-configured"
                    };
                    let reason = p["configured_reason"].as_str().unwrap_or("");
                    println!(
                        "{}  {}  {}  {}{}",
                        p["id"].as_str().unwrap_or("?"),
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
                bail!(
                    "usage: clotho provider connect <id> --api-key <key> [--org <org>]"
                );
            }
            let id = args.remove(0);
            let api_key = take_option(&mut args, "--api-key")
                .context("provider API key required: --api-key <key>")?;
            let org = take_option(&mut args, "--org").unwrap_or_default();
            let body = request_value(
                config,
                reqwest::Method::POST,
                &format!("/api/v1/providers/{id}/connect"),
                Some(json!({ "api_key": api_key, "org": org })),
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
            let value =
                take_option(&mut args, "--value").context("secret value required: --value <val>")?;
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
            let body =
                request_value(config, reqwest::Method::DELETE, &path, None).await?;
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
                request_value(config, reqwest::Method::POST, "/api/v1/orgs", Some(payload))
                    .await?;
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
  clotho [--api <url>] [--json] <command> ...

  Global:
    --api <url>     API gateway (default $CLOTHO_API_URL or http://localhost:8080)
    --json          Machine-readable JSON on stdout

  repo
    clotho repo init <name>
    clotho repo list
    clotho repo status <repo>
    clotho repo log <repo>
    clotho repo tree <repo>
    clotho repo commit <repo> -m <msg> --file <path> [...] [--submit]
    clotho repo submit <repo> <commit-id>

  issue
    clotho issue list <repo> [open|closed|all]
    clotho issue create <repo> --title <t> [--body <b>]
    clotho issue get <repo> <number>
    clotho issue comment <repo> <number> --body <text>

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
    clotho actions config <repo> [--provider <id>] [--enabled true|false]

  provider
    clotho provider list
    clotho provider get <id>
    clotho provider connect <id> --api-key <key> [--org <org>]

  secret   (values are write-only; responses are metadata + last4)
    clotho secret list org|repo <name>
    clotho secret set org|repo <owner> --name <n> --value <v>
    clotho secret get org|repo <owner> <secret-name>
    clotho secret delete org|repo <owner> <secret-name>

  org / activity
    clotho org list|create|get|repos ...
    clotho activity [--limit N]

  Stage 8 aliases (still work): init, status, log, commit, submit, pr <repo>"
    );
}
