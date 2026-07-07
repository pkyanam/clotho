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

/// The subset of Forgejo's repository object the gateway returns to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub id: i64,
    pub full_name: String,
    pub html_url: String,
    pub default_branch: String,
    #[serde(default)]
    pub has_issues: bool,
    #[serde(default)]
    pub has_pull_requests: bool,
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
        let url = format!("{}{path}", self.config.base_url.trim_end_matches('/'));
        let token = self.token().await?;
        let response = self
            .http
            .request(method, &url)
            .header("Authorization", format!("token {token}"))
            .send()
            .await
            .map_err(|e| ApiError::Upstream(format!("forgejo: {e}")))?;
        if response.status().is_success() {
            Ok(response)
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(ApiError::Upstream(format!(
                "forgejo: {path} returned {status}: {body}"
            )))
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
        self.request(
            reqwest::Method::GET,
            &format!("/api/v1/repos/{owner}/{name}"),
        )
        .await?
        .json()
        .await
        .map_err(|e| ApiError::Upstream(format!("forgejo: invalid repo response: {e}")))
    }
}
