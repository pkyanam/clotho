//! `clotho` CLI: a thin human-facing client over the api-gateway REST edge.
//! It never shells out to git or jj; local files are read directly and sent to
//! the gateway, where the VCS engine writes real commits.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug)]
struct Config {
    api_url: String,
}

#[derive(Debug, Deserialize)]
struct CreatedRepo {
    name: String,
    owner: String,
    initial_commit_id: String,
}

#[derive(Debug, Deserialize)]
struct RepoDetail {
    name: String,
    owner: String,
    main_commit_id: String,
    heads: Vec<Commit>,
}

#[derive(Debug, Deserialize)]
struct Commit {
    commit_id: String,
    description: String,
    author_name: String,
    timestamp_millis: i64,
}

#[derive(Debug, Deserialize)]
struct CreatedCommit {
    commit_id: String,
    change_id: String,
    operation_id: String,
}

#[derive(Debug, Deserialize)]
struct SubmitResult {
    commit_id: String,
    operation_id: String,
    fast_forwarded: bool,
    conflicted: bool,
    conflicted_paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Pull {
    number: i64,
    title: String,
    state: String,
    html_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let api_url = take_global_option(&mut args, "--api")
        .or_else(|| std::env::var("CLOTHO_API_URL").ok())
        .unwrap_or_else(|| "http://localhost:8080".into());
    let config = Config {
        api_url: api_url.trim_end_matches('/').to_string(),
    };

    let Some(command) = args.first().cloned() else {
        usage();
        return Ok(());
    };
    args.remove(0);

    match command.as_str() {
        "init" => cmd_init(&config, args).await,
        "status" => cmd_status(&config, args).await,
        "log" => cmd_log(&config, args).await,
        "commit" => cmd_commit(&config, args).await,
        "submit" => cmd_submit(&config, args).await,
        "pr" => cmd_pr(&config, args).await,
        "help" | "-h" | "--help" => {
            usage();
            Ok(())
        }
        other => bail!("unknown command {other:?}; run `clotho help`"),
    }
}

async fn cmd_init(config: &Config, args: Vec<String>) -> Result<()> {
    let [name] = one_or_usage(args, "clotho init <repo>")?;
    let repo: CreatedRepo = request_json(
        config,
        reqwest::Method::POST,
        "/api/v1/repos",
        Some(json!({ "name": name })),
    )
    .await?;
    println!(
        "created {}/{} at {}",
        repo.owner, repo.name, repo.initial_commit_id
    );
    Ok(())
}

async fn cmd_status(config: &Config, args: Vec<String>) -> Result<()> {
    let [repo] = one_or_usage(args, "clotho status <repo>")?;
    let detail: RepoDetail = request_json(
        config,
        reqwest::Method::GET,
        &format!("/api/v1/repos/{repo}"),
        None,
    )
    .await?;
    let tree: serde_json::Value = request_json(
        config,
        reqwest::Method::GET,
        &format!("/api/v1/repos/{repo}/tree"),
        None,
    )
    .await?;
    let files = tree["files"].as_array().map_or(0, Vec::len);
    println!("{}/{}", detail.owner, detail.name);
    println!("main {}", short(&detail.main_commit_id));
    println!("heads {}", detail.heads.len());
    println!("files {files}");
    Ok(())
}

async fn cmd_log(config: &Config, args: Vec<String>) -> Result<()> {
    let [repo] = one_or_usage(args, "clotho log <repo>")?;
    let body: serde_json::Value = request_json(
        config,
        reqwest::Method::GET,
        &format!("/api/v1/repos/{repo}/commits?limit=20"),
        None,
    )
    .await?;
    let commits: Vec<Commit> = serde_json::from_value(body["commits"].clone())?;
    for commit in commits {
        println!(
            "{} {} {} {}",
            short(&commit.commit_id),
            commit.timestamp_millis,
            commit.author_name,
            first_line(&commit.description)
        );
    }
    Ok(())
}

async fn cmd_commit(config: &Config, mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("usage: clotho commit <repo> -m <message> --file <path> [--file <path> ...] [--delete <path>] [--submit]");
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
    for path in files {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("read text file {}", path.display()))?;
        file_payloads.push(json!({
            "path": repo_path(&path),
            "content": content,
            "executable": is_executable(&path),
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

    let commit: CreatedCommit = request_json(
        config,
        reqwest::Method::POST,
        &format!("/api/v1/repos/{repo}/commits"),
        Some(body),
    )
    .await?;
    println!(
        "committed {} change {} op {}",
        commit.commit_id, commit.change_id, commit.operation_id
    );

    if submit {
        let landed = submit_commit(config, &repo, &commit.commit_id).await?;
        print_submit(&landed);
    }
    Ok(())
}

async fn cmd_submit(config: &Config, args: Vec<String>) -> Result<()> {
    if args.len() != 2 {
        bail!("usage: clotho submit <repo> <commit-id>");
    }
    let landed = submit_commit(config, &args[0], &args[1]).await?;
    print_submit(&landed);
    Ok(())
}

async fn cmd_pr(config: &Config, args: Vec<String>) -> Result<()> {
    if args.is_empty() || args.len() > 2 {
        bail!("usage: clotho pr <repo> [open|closed|all]");
    }
    let repo = &args[0];
    let state = args.get(1).map(String::as_str).unwrap_or("open");
    if !matches!(state, "open" | "closed" | "all") {
        bail!("pr state must be open, closed, or all");
    }
    let body: serde_json::Value = request_json(
        config,
        reqwest::Method::GET,
        &format!("/api/v1/repos/{repo}/pulls?state={state}"),
        None,
    )
    .await?;
    let pulls: Vec<Pull> = serde_json::from_value(body["pulls"].clone())?;
    for pull in pulls {
        println!(
            "#{} {} {} {}",
            pull.number, pull.state, pull.title, pull.html_url
        );
    }
    Ok(())
}

async fn submit_commit(config: &Config, repo: &str, commit_id: &str) -> Result<SubmitResult> {
    request_json(
        config,
        reqwest::Method::POST,
        &format!("/api/v1/repos/{repo}/submit"),
        Some(json!({ "commit_id": commit_id })),
    )
    .await
}

fn print_submit(result: &SubmitResult) {
    println!(
        "landed {} op {} fast_forwarded={} conflicted={}",
        result.commit_id, result.operation_id, result.fast_forwarded, result.conflicted
    );
    if !result.conflicted_paths.is_empty() {
        println!("conflicts {}", result.conflicted_paths.join(", "));
    }
}

async fn request_json<T: serde::de::DeserializeOwned>(
    config: &Config,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<T> {
    let client = reqwest::Client::new();
    let mut request = client.request(method.clone(), format!("{}{}", config.api_url, path));
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        bail!("{method} {path} failed: {status}: {text}");
    }
    serde_json::from_str(&text).with_context(|| format!("decode response from {path}"))
}

fn take_global_option(args: &mut Vec<String>, name: &str) -> Option<String> {
    take_option(args, name)
}

fn take_option(args: &mut Vec<String>, name: &str) -> Option<String> {
    let pos = args.iter().position(|a| a == name)?;
    args.remove(pos);
    if pos >= args.len() {
        return None;
    }
    Some(args.remove(pos))
}

fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(pos) = args.iter().position(|a| a == name) {
        args.remove(pos);
        true
    } else {
        false
    }
}

fn take_repeated(args: &mut Vec<String>, name: &str) -> Vec<PathBuf> {
    let mut values = Vec::new();
    while let Some(pos) = args.iter().position(|a| a == name) {
        args.remove(pos);
        if pos < args.len() {
            values.push(PathBuf::from(args.remove(pos)));
        }
    }
    values
}

fn one_or_usage<const N: usize>(args: Vec<String>, usage: &str) -> Result<[String; N]> {
    args.try_into()
        .map_err(|_| anyhow::anyhow!("usage: {usage}"))
}

fn repo_path(path: &std::path::Path) -> String {
    path.to_string_lossy().trim_start_matches("./").to_string()
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &std::path::Path) -> bool {
    false
}

fn short(commit_id: &str) -> &str {
    commit_id.get(..12).unwrap_or(commit_id)
}

fn first_line(message: &str) -> &str {
    message.lines().next().unwrap_or("")
}

fn usage() {
    eprintln!(
        "usage:
  clotho [--api <url>] init <repo>
  clotho [--api <url>] status <repo>
  clotho [--api <url>] log <repo>
  clotho [--api <url>] commit <repo> -m <message> --file <path> [--file <path> ...] [--delete <path>] [--submit]
  clotho [--api <url>] submit <repo> <commit-id>
  clotho [--api <url>] pr <repo> [open|closed|all]"
    );
}
