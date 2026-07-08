//! Repo-browser read endpoints: repo list/detail, tree, file contents,
//! commit log, and the jj operation log — clotho-vcs (and Forgejo, for the
//! project entry) aggregated into the JSON the web app renders (ADR-0007).
//!
//! Stage 11 makes the repo list/detail query Clotho's control-plane tables
//! first, then overlays Forgejo collaboration metadata. Without a database,
//! the handlers fall back to the Stage 3/6 Forgejo behavior.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use clotho_common::pb::vcs::v1::{
    CommitSummary, GetFileRequest, GetHeadsRequest, ListFilesRequest, LogCommitsRequest,
    QueryOpLogRequest,
};
use serde::{Deserialize, Serialize};

use crate::control;
use crate::error::ApiError;
use crate::forgejo::RepoInfo;
use crate::AppState;

#[derive(Serialize)]
pub struct RepoListResponse {
    pub repos: Vec<RepoInfo>,
}

pub async fn list_repos(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RepoListResponse>, ApiError> {
    let provider = state.actions.default_provider();
    let configured = state.actions.provider_configured(&provider);
    let base_url = state.forgejo.config().base_url.clone();

    let mut repos = if let Some(pool) = &state.pool {
        let clotho = control::list_repos_with_orgs(pool).await?;
        let forgejo_by_name: HashMap<String, RepoInfo> = state
            .forgejo
            .list_repos()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| (r.name.clone(), r))
            .collect();
        clotho
            .into_iter()
            .map(|c| {
                control::build_repo_info(
                    &c,
                    forgejo_by_name.get(&c.repo.name),
                    &base_url,
                    &provider,
                    configured,
                )
            })
            .collect()
    } else {
        vec![]
    };

    if repos.is_empty() {
        // Fallback to Forgejo when the control-plane table is empty or the
        // gateway is running without a database.
        repos = state
            .forgejo
            .list_repos()
            .await?
            .into_iter()
            .map(|mut r| {
                r.provider = provider.clone();
                r.configured = configured;
                r
            })
            .collect();
    }

    Ok(Json(RepoListResponse { repos }))
}

#[derive(Serialize)]
pub struct CommitJson {
    pub commit_id: String,
    pub change_id: String,
    pub description: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp_millis: i64,
    pub parent_commit_ids: Vec<String>,
}

impl From<CommitSummary> for CommitJson {
    fn from(c: CommitSummary) -> Self {
        Self {
            commit_id: c.commit_id,
            change_id: c.change_id,
            description: c.description,
            author_name: c.author_name,
            author_email: c.author_email,
            timestamp_millis: c.timestamp_millis,
            parent_commit_ids: c.parent_commit_ids,
        }
    }
}

#[derive(Serialize)]
pub struct RepoDetailResponse {
    pub name: String,
    /// Forgejo/clone-path owner.
    pub owner: String,
    /// Clotho org that owns this repo.
    pub owner_org: String,
    pub visibility: String,
    pub default_branch: String,
    pub clone_url: String,
    pub provider: String,
    pub configured: bool,
    pub forgejo: RepoInfo,
    /// Commit the `main` bookmark points at; empty while main is unborn.
    pub main_commit_id: String,
    /// All current head commits — jj keeps concurrent agents' anonymous
    /// heads first-class, so this is the live "who is working" picture.
    pub heads: Vec<CommitJson>,
}

pub async fn get_repo(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<RepoDetailResponse>, ApiError> {
    let provider = state.actions.default_provider();
    let configured = state.actions.provider_configured(&provider);
    let base_url = state.forgejo.config().base_url.clone();

    // Stage 11: look up the Clotho record first.
    let (repo_info, owner, owner_org, visibility, default_branch, clone_url) =
        if let Some(pool) = &state.pool {
            match control::get_repo_with_org(pool, &name).await? {
                Some(clotho) => {
                    let forgejo = state.forgejo.get_repo(&name).await.ok();
                    let mut info = control::build_repo_info(
                        &clotho,
                        forgejo.as_ref(),
                        &base_url,
                        &provider,
                        configured,
                    );
                    info.owner = clotho.repo.forgejo_owner.clone();
                    (
                        info,
                        clotho.repo.forgejo_owner.clone(),
                        clotho.org_name,
                        clotho.repo.visibility.clone(),
                        clotho.repo.default_branch.clone(),
                        format!(
                            "{}/{}/{name}.git",
                            base_url.trim_end_matches('/'),
                            clotho.repo.forgejo_owner
                        ),
                    )
                }
                None => {
                    // Not in the control-plane table yet: fall back to the
                    // Stage 3/6 behavior so existing repos stay reachable.
                    let forgejo_repo = state.forgejo.get_repo(&name).await?;
                    let mut forgejo = forgejo_repo;
                    forgejo.provider = provider.clone();
                    forgejo.configured = configured;
                    let owner = state.forgejo.owner().to_string();
                    let clone = format!("{}/{}/{name}.git", base_url.trim_end_matches('/'), owner);
                    (
                        forgejo,
                        owner.clone(),
                        owner,
                        "public".into(),
                        "main".into(),
                        clone,
                    )
                }
            }
        } else {
            let forgejo_repo = state.forgejo.get_repo(&name).await?;
            let mut forgejo = forgejo_repo;
            forgejo.provider = provider.clone();
            forgejo.configured = configured;
            let owner = state.forgejo.owner().to_string();
            let clone = format!("{}/{}/{name}.git", base_url.trim_end_matches('/'), owner);
            (
                forgejo,
                owner.clone(),
                owner,
                "public".into(),
                "main".into(),
                clone,
            )
        };

    let mut vcs = state.vcs.clone();
    let heads = vcs
        .get_heads(GetHeadsRequest { repo: name.clone() })
        .await?
        .into_inner();
    Ok(Json(RepoDetailResponse {
        name,
        owner,
        owner_org,
        visibility,
        default_branch,
        clone_url,
        provider,
        configured,
        forgejo: repo_info,
        main_commit_id: heads.main_commit_id,
        heads: heads.heads.into_iter().map(Into::into).collect(),
    }))
}

#[derive(Deserialize)]
pub struct TreeQuery {
    #[serde(default)]
    pub commit_id: String,
}

#[derive(Serialize)]
pub struct TreeEntryJson {
    pub path: String,
    pub size_bytes: u64,
    pub executable: bool,
    pub conflicted: bool,
}

#[derive(Serialize)]
pub struct TreeResponse {
    pub commit_id: String,
    pub files: Vec<TreeEntryJson>,
}

pub async fn tree(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<TreeQuery>,
) -> Result<Json<TreeResponse>, ApiError> {
    let mut vcs = state.vcs.clone();
    let list = vcs
        .list_files(ListFilesRequest {
            repo: name,
            commit_id: query.commit_id,
        })
        .await?
        .into_inner();
    Ok(Json(TreeResponse {
        commit_id: list.commit_id,
        files: list
            .files
            .into_iter()
            .map(|f| TreeEntryJson {
                path: f.path,
                size_bytes: f.size_bytes,
                executable: f.executable,
                conflicted: f.conflicted,
            })
            .collect(),
    }))
}

#[derive(Deserialize)]
pub struct FileQuery {
    pub path: String,
    #[serde(default)]
    pub commit_id: String,
}

#[derive(Serialize)]
pub struct FileResponse {
    pub commit_id: String,
    pub path: String,
    pub executable: bool,
    /// The entry is an unresolved jj conflict; `content` holds its
    /// materialized conflict-marker text (ADR-0006).
    pub conflicted: bool,
    pub size_bytes: u64,
    /// UTF-8 text contents; `null` when the file is binary.
    pub content: Option<String>,
    pub binary: bool,
}

pub async fn file(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<FileQuery>,
) -> Result<Json<FileResponse>, ApiError> {
    let mut vcs = state.vcs.clone();
    let file = vcs
        .get_file(GetFileRequest {
            repo: name,
            commit_id: query.commit_id,
            path: query.path,
        })
        .await?
        .into_inner();
    let size_bytes = file.content.len() as u64;
    let content = String::from_utf8(file.content).ok();
    Ok(Json(FileResponse {
        commit_id: file.commit_id,
        path: file.path,
        executable: file.executable,
        conflicted: file.conflicted,
        size_bytes,
        binary: content.is_none(),
        content,
    }))
}

#[derive(Deserialize)]
pub struct CommitsQuery {
    #[serde(default)]
    pub from_commit_id: String,
    #[serde(default = "default_commits_limit")]
    pub limit: u32,
}

fn default_commits_limit() -> u32 {
    50
}

#[derive(Serialize)]
pub struct CommitsResponse {
    pub commits: Vec<CommitJson>,
}

pub async fn commits(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<CommitsQuery>,
) -> Result<Json<CommitsResponse>, ApiError> {
    let mut vcs = state.vcs.clone();
    let log = vcs
        .log_commits(LogCommitsRequest {
            repo: name,
            from_commit_id: query.from_commit_id,
            limit: query.limit.clamp(1, 500),
        })
        .await?
        .into_inner();
    Ok(Json(CommitsResponse {
        commits: log.commits.into_iter().map(Into::into).collect(),
    }))
}

#[derive(Deserialize)]
pub struct OpLogQuery {
    #[serde(default = "default_op_log_limit")]
    pub limit: u32,
}

fn default_op_log_limit() -> u32 {
    50
}

#[derive(Serialize)]
pub struct OpLogEntryJson {
    pub operation_id: String,
    pub description: String,
    pub start_time_millis: i64,
    pub end_time_millis: i64,
    pub parent_operation_ids: Vec<String>,
}

#[derive(Serialize)]
pub struct OpLogResponse {
    pub entries: Vec<OpLogEntryJson>,
}

pub async fn op_log(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<OpLogQuery>,
) -> Result<Json<OpLogResponse>, ApiError> {
    let mut vcs = state.vcs.clone();
    let log = vcs
        .query_op_log(QueryOpLogRequest {
            repo: name,
            limit: query.limit.clamp(1, 500),
        })
        .await?
        .into_inner();
    Ok(Json(OpLogResponse {
        entries: log
            .entries
            .into_iter()
            .map(|e| OpLogEntryJson {
                operation_id: e.operation_id,
                description: e.description,
                start_time_millis: e.start_time_millis,
                end_time_millis: e.end_time_millis,
                parent_operation_ids: e.parent_operation_ids,
            })
            .collect(),
    }))
}
