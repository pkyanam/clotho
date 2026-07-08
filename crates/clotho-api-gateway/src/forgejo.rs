//! Minimal Forgejo REST client — only what the gateway needs.
//!
//! Clotho talks to Forgejo strictly over its HTTP API (the GPLv3 boundary,
//! see collab/README.md): no Forgejo code is linked, vendored, or modified.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::ApiError;

#[derive(Clone)]
pub struct ForgejoConfig {
    /// Base URL, e.g. `http://forgejo:3000`.
    pub base_url: String,
    /// The Forgejo user that owns Clotho-provisioned repos (the admin user
    /// created by scripts/forgejo/provision.sh in dev).
    pub owner: String,
    pub token: TokenSource,
}

/// Where the API token comes from. In the dev stack the one-shot provisioner
/// mints it at first boot and drops it in a shared volume, so it can't be
/// baked into an env var ahead of time.
#[derive(Clone)]
pub enum TokenSource {
    Inline(String),
    File(PathBuf),
}

/// Clotho-owned repository summary. In Stage 11 this is the control-plane
/// record surfaced to clients, with Forgejo collaboration metadata overlaid.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub id: i64,
    #[serde(default)]
    pub clotho_id: String,
    pub name: String,
    pub full_name: String,
    #[serde(default, skip_deserializing)]
    pub owner: String,
    pub html_url: String,
    #[serde(default)]
    pub clone_url: String,
    pub default_branch: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub visibility: String,
    #[serde(default)]
    pub has_issues: bool,
    #[serde(default)]
    pub has_pull_requests: bool,
    #[serde(default)]
    pub open_issues_count: i64,
    #[serde(default)]
    pub open_pr_counter: i64,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub configured: bool,
}

/// One endpoint of a pull request (its head or base).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: String,
}

/// Clotho-owned pull-request summary, backed by Forgejo in Stage 9.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullInfo {
    pub number: i64,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    pub state: String,
    pub user: PullUser,
    pub head: PullRef,
    pub base: PullRef,
    /// Common ancestor of head and base — the diff base for "what this PR
    /// introduces" (same semantics as a three-dot diff).
    #[serde(default)]
    pub merge_base: String,
    #[serde(default)]
    pub merged: bool,
    pub mergeable: bool,
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub comments: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullUser {
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueInfo {
    pub number: i64,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    pub state: String,
    pub user: PullUser,
    #[serde(default)]
    pub labels: Vec<IssueLabel>,
    #[serde(default)]
    pub comments: i64,
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueLabel {
    pub name: String,
    #[serde(default)]
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentInfo {
    pub id: i64,
    #[serde(default)]
    pub body: String,
    pub user: PullUser,
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub commit: BranchCommit,
    #[serde(default)]
    pub protected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchCommit {
    pub id: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitStatusInfo {
    pub id: i64,
    pub state: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub target_url: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct ForgejoClient {
    config: ForgejoConfig,
    http: reqwest::Client,
}

impl ForgejoClient {
    pub fn new(config: ForgejoConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    pub fn owner(&self) -> &str {
        &self.config.owner
    }

    pub fn config(&self) -> &ForgejoConfig {
        &self.config
    }

    async fn token(&self) -> Result<String, ApiError> {
        match &self.config.token {
            TokenSource::Inline(token) => Ok(token.clone()),
            TokenSource::File(path) => {
                let raw = tokio::fs::read_to_string(path).await.map_err(|e| {
                    ApiError::Internal(format!("forgejo token file {}: {e}", path.display()))
                })?;
                Ok(raw.trim().to_string())
            }
        }
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::Response, ApiError> {
        self.request_with_body(method, path, None).await
    }

    async fn request_with_body(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, ApiError> {
        let url = format!("{}{path}", self.config.base_url.trim_end_matches('/'));
        let token = self.token().await?;
        let mut builder = self
            .http
            .request(method, &url)
            .header("Authorization", format!("token {token}"));
        if let Some(body) = body {
            builder = builder.json(&body);
        }
        let response = builder
            .send()
            .await
            .map_err(|e| ApiError::Upstream(format!("forgejo: {e}")))?;
        if response.status().is_success() {
            Ok(response)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let message = format!("forgejo: {path} returned {status}: {body}");
            if status == reqwest::StatusCode::NOT_FOUND {
                Err(ApiError::NotFound(message))
            } else {
                Err(ApiError::Upstream(message))
            }
        }
    }

    /// Adopt the bare git repo clotho-vcs wrote at `<owner>/<name>.git` under
    /// Forgejo's repository root, turning it into a full Forgejo project
    /// (issues, PRs) without Forgejo ever owning the git objects.
    pub async fn adopt_repo(&self, name: &str) -> Result<RepoInfo, ApiError> {
        let owner = &self.config.owner;
        self.request(
            reqwest::Method::POST,
            &format!("/api/v1/admin/unadopted/{owner}/{name}"),
        )
        .await?;
        self.get_repo(name).await
    }

    pub async fn get_repo(&self, name: &str) -> Result<RepoInfo, ApiError> {
        let owner = &self.config.owner;
        self.get_json(&format!("/api/v1/repos/{owner}/{name}"))
            .await
    }

    /// All repos owned by the Clotho owner user, newest activity first.
    pub async fn list_repos(&self) -> Result<Vec<RepoInfo>, ApiError> {
        let owner = &self.config.owner;
        self.get_json(&format!(
            "/api/v1/users/{owner}/repos?limit=50&sort=updated&order=desc"
        ))
        .await
    }

    /// Pull requests on a repo. `state` is `open`, `closed`, or `all`.
    pub async fn list_pulls(&self, name: &str, state: &str) -> Result<Vec<PullInfo>, ApiError> {
        let owner = &self.config.owner;
        self.get_json(&format!(
            "/api/v1/repos/{owner}/{name}/pulls?state={state}&sort=recentupdate&limit=50"
        ))
        .await
    }

    pub async fn get_pull(&self, name: &str, number: i64) -> Result<PullInfo, ApiError> {
        let owner = &self.config.owner;
        self.get_json(&format!("/api/v1/repos/{owner}/{name}/pulls/{number}"))
            .await
    }

    pub async fn create_pull(
        &self,
        name: &str,
        title: &str,
        body: &str,
        head: &str,
        base: &str,
    ) -> Result<PullInfo, ApiError> {
        let owner = &self.config.owner;
        let body = serde_json::json!({
            "title": title,
            "body": body,
            "head": head,
            "base": base,
        });
        self.post_json(&format!("/api/v1/repos/{owner}/{name}/pulls"), body)
            .await
    }

    pub async fn comment_on_pull(
        &self,
        name: &str,
        number: i64,
        body: &str,
    ) -> Result<CommentInfo, ApiError> {
        self.comment_on_issue(name, number, body).await
    }

    pub async fn review_pull(
        &self,
        name: &str,
        number: i64,
        body: &str,
        event: &str,
    ) -> Result<CommentInfo, ApiError> {
        let owner = &self.config.owner;
        let payload = serde_json::json!({
            "body": body,
            "event": event,
        });
        self.post_json(
            &format!("/api/v1/repos/{owner}/{name}/pulls/{number}/reviews"),
            payload,
        )
        .await
    }

    pub async fn merge_pull(
        &self,
        name: &str,
        number: i64,
        merge_method: &str,
        title: Option<&str>,
        message: Option<&str>,
    ) -> Result<PullInfo, ApiError> {
        let owner = &self.config.owner;
        let payload = serde_json::json!({
            "Do": merge_method,
            "MergeTitleField": title.unwrap_or(""),
            "MergeMessageField": message.unwrap_or(""),
        });
        self.request_with_body(
            reqwest::Method::POST,
            &format!("/api/v1/repos/{owner}/{name}/pulls/{number}/merge"),
            Some(payload),
        )
        .await?;
        self.get_pull(name, number).await
    }

    pub async fn list_issues(&self, name: &str, state: &str) -> Result<Vec<IssueInfo>, ApiError> {
        let owner = &self.config.owner;
        self.get_json(&format!(
            "/api/v1/repos/{owner}/{name}/issues?state={state}&type=issues&sort=recentupdate&limit=50"
        ))
        .await
    }

    pub async fn create_issue(
        &self,
        name: &str,
        title: &str,
        body: &str,
    ) -> Result<IssueInfo, ApiError> {
        let owner = &self.config.owner;
        let payload = serde_json::json!({
            "title": title,
            "body": body,
        });
        self.post_json(&format!("/api/v1/repos/{owner}/{name}/issues"), payload)
            .await
    }

    pub async fn get_issue(&self, name: &str, number: i64) -> Result<IssueInfo, ApiError> {
        let owner = &self.config.owner;
        self.get_json(&format!("/api/v1/repos/{owner}/{name}/issues/{number}"))
            .await
    }

    pub async fn list_issue_comments(
        &self,
        name: &str,
        number: i64,
    ) -> Result<Vec<CommentInfo>, ApiError> {
        let owner = &self.config.owner;
        self.get_json(&format!(
            "/api/v1/repos/{owner}/{name}/issues/{number}/comments?limit=100"
        ))
        .await
    }

    pub async fn comment_on_issue(
        &self,
        name: &str,
        number: i64,
        body: &str,
    ) -> Result<CommentInfo, ApiError> {
        let owner = &self.config.owner;
        let payload = serde_json::json!({ "body": body });
        self.post_json(
            &format!("/api/v1/repos/{owner}/{name}/issues/{number}/comments"),
            payload,
        )
        .await
    }

    pub async fn list_branches(&self, name: &str) -> Result<Vec<BranchInfo>, ApiError> {
        let owner = &self.config.owner;
        self.get_json(&format!("/api/v1/repos/{owner}/{name}/branches?limit=100"))
            .await
    }

    pub async fn commit_statuses(
        &self,
        name: &str,
        sha: &str,
    ) -> Result<Vec<CommitStatusInfo>, ApiError> {
        let owner = &self.config.owner;
        self.get_json(&format!("/api/v1/repos/{owner}/{name}/statuses/{sha}"))
            .await
    }

    /// Attach a commit status to `sha` — how Stage 7 CI reports back to the PR
    /// (`state` is one of `pending`, `success`, `failure`, `error`).
    pub async fn set_commit_status(
        &self,
        name: &str,
        sha: &str,
        state: &str,
        context: &str,
        description: &str,
        target_url: &str,
    ) -> Result<(), ApiError> {
        let owner = &self.config.owner;
        let body = serde_json::json!({
            "state": state,
            "context": context,
            "description": description,
            "target_url": target_url,
        });
        self.request_with_body(
            reqwest::Method::POST,
            &format!("/api/v1/repos/{owner}/{name}/statuses/{sha}"),
            Some(body),
        )
        .await?;
        Ok(())
    }

    /// Register a push webhook on the repo pointing back at the gateway, so
    /// pushes trigger CI (docs/adr/0008). Idempotency is best-effort: a
    /// duplicate hook just means CI fires twice, harmless for the prototype.
    pub async fn create_push_webhook(
        &self,
        name: &str,
        webhook_url: &str,
        secret: &str,
    ) -> Result<(), ApiError> {
        let owner = &self.config.owner;
        let body = serde_json::json!({
            "type": "gitea",
            "active": true,
            "events": ["push"],
            "config": {
                "url": webhook_url,
                "content_type": "json",
                "secret": secret,
            },
        });
        self.request_with_body(
            reqwest::Method::POST,
            &format!("/api/v1/repos/{owner}/{name}/hooks"),
            Some(body),
        )
        .await?;
        Ok(())
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        self.request(reqwest::Method::GET, path)
            .await?
            .json()
            .await
            .map_err(|e| ApiError::Upstream(format!("forgejo: invalid response for {path}: {e}")))
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, ApiError> {
        self.request_with_body(reqwest::Method::POST, path, Some(body))
            .await?
            .json()
            .await
            .map_err(|e| ApiError::Upstream(format!("forgejo: invalid response for {path}: {e}")))
    }
}
