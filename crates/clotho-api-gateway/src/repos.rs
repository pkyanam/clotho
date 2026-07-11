//! Repo-browser read endpoints: repo list/detail, tree, file contents,
//! commit log, and the jj operation log — clotho-vcs aggregated into the
//! JSON the web app renders (ADR-0007).
//!
//! Stage 11 makes the repo list/detail query Clotho's control-plane tables
//! first, then overlays collaboration metadata. Slice A adds PATCH/DELETE for
//! repo settings.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use base64::Engine;
use clotho_common::pb::storage::v1::GetStorageStatsRequest;
use clotho_common::pb::vcs::v1::{
    CommitSummary, GetFileRequest, GetHeadsRequest, ListFilesRequest, LogCommitsRequest,
    QueryOpLogRequest,
};
use serde::{Deserialize, Serialize};

use crate::auth;
use crate::control::{self, ActivityEventInput, UpdateRepoRequest};
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
    let base_url = state.public_git_url.clone();

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
    /// Git clone-path owner.
    pub owner: String,
    /// Clotho org that owns this repo.
    pub owner_org: String,
    pub description: String,
    pub visibility: String,
    pub default_branch: String,
    pub clone_url: String,
    pub provider: String,
    pub configured: bool,
    pub info: RepoInfo,
    /// Commit the `main` bookmark points at; empty while main is unborn.
    pub main_commit_id: String,
    /// All current head commits — jj keeps concurrent agents' anonymous
    /// heads first-class, so this is the live "who is working" picture.
    pub heads: Vec<CommitJson>,
}

fn public_clone_url(state: &AppState, git_owner: &str, name: &str) -> String {
    let base = state.public_git_url.trim_end_matches('/');
    format!("{base}/{git_owner}/{name}.git")
}

async fn load_repo_detail(
    state: &AppState,
    name: &str,
) -> Result<(RepoInfo, String, String, String, String, String, String), ApiError> {
    let provider = state.actions.default_provider();
    let configured = state.actions.provider_configured(&provider);
    let base_url = state.public_git_url.clone();

    if let Some(pool) = &state.pool {
        match control::get_repo_with_org(pool, name).await? {
            Some(clotho) => {
                let forgejo = state.forgejo.get_repo(name).await.ok();
                let mut info = control::build_repo_info(
                    &clotho,
                    forgejo.as_ref(),
                    &base_url,
                    &provider,
                    configured,
                );
                info.owner = clotho.repo.forgejo_owner.clone();
                let clone = public_clone_url(state, &clotho.repo.forgejo_owner, name);
                Ok((
                    info,
                    clotho.repo.forgejo_owner.clone(),
                    clotho.org_name,
                    clotho.repo.description.clone(),
                    clotho.repo.visibility.clone(),
                    clotho.repo.default_branch.clone(),
                    clone,
                ))
            }
            None => {
                let forgejo_repo = state.forgejo.get_repo(name).await?;
                let mut info = forgejo_repo;
                info.provider = provider.clone();
                info.configured = configured;
                let owner = state.forgejo.owner().to_string();
                let clone = public_clone_url(state, &owner, name);
                Ok((
                    info.clone(),
                    owner.clone(),
                    owner,
                    info.description.clone(),
                    info.visibility.clone(),
                    info.default_branch.clone(),
                    clone,
                ))
            }
        }
    } else {
        let forgejo_repo = state.forgejo.get_repo(name).await?;
        let mut info = forgejo_repo;
        info.provider = provider.clone();
        info.configured = configured;
        let owner = state.forgejo.owner().to_string();
        let clone = public_clone_url(state, &owner, name);
        Ok((
            info.clone(),
            owner.clone(),
            owner,
            info.description.clone(),
            info.visibility.clone(),
            info.default_branch.clone(),
            clone,
        ))
    }
}

pub async fn get_repo(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<RepoDetailResponse>, ApiError> {
    let (repo_info, owner, owner_org, description, visibility, default_branch, clone_url) =
        load_repo_detail(&state, &name).await?;

    let mut vcs = state.vcs.clone();
    let heads = vcs
        .get_heads(GetHeadsRequest { repo: name.clone() })
        .await?
        .into_inner();
    Ok(Json(RepoDetailResponse {
        name,
        owner,
        owner_org,
        description,
        visibility,
        default_branch,
        clone_url,
        provider: state.actions.default_provider(),
        configured: state
            .actions
            .provider_configured(&state.actions.default_provider()),
        info: repo_info,
        main_commit_id: heads.main_commit_id,
        heads: heads.heads.into_iter().map(Into::into).collect(),
    }))
}

pub async fn update_repo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(req): Json<UpdateRepoRequest>,
) -> Result<Json<RepoDetailResponse>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    if let Some(pool) = &state.pool {
        let clotho = control::require_repo_admin(pool, &name, &auth.user_id).await?;
        let updated = control::update_repo_row(pool, &clotho.repo.id, &req).await?;

        if let Err(e) = state
            .forgejo
            .patch_repo(
                &updated.forgejo_owner,
                &updated.name,
                req.description.as_deref(),
                req.visibility.as_deref(),
                req.default_branch.as_deref(),
            )
            .await
        {
            tracing::warn!(repo = %name, error = %e, "best-effort collab provider repo sync failed");
        }

        control::log_activity(
            pool,
            ActivityEventInput {
                actor_id: auth.user_id.clone(),
                org_id: Some(clotho.repo.org_id.clone()),
                repo_id: Some(clotho.repo.id.clone()),
                event_type: "repo.updated".into(),
                payload: serde_json::json!({
                    "repo_name": name,
                    "description": req.description,
                    "visibility": req.visibility,
                    "default_branch": req.default_branch,
                }),
            },
        )
        .await?;

        let clotho_with_org = control::get_repo_with_org(pool, &name)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("repo {name:?} not found")))?;
        let provider = state.actions.default_provider();
        let configured = state.actions.provider_configured(&provider);
        let mut info = control::build_repo_info(
            &clotho_with_org,
            None,
            &state.public_git_url,
            &provider,
            configured,
        );
        info.owner = clotho_with_org.repo.forgejo_owner.clone();
        let clone_url = public_clone_url(&state, &clotho_with_org.repo.forgejo_owner, &name);
        Ok(Json(RepoDetailResponse {
            name: clotho_with_org.repo.name.clone(),
            owner: clotho_with_org.repo.forgejo_owner.clone(),
            owner_org: clotho_with_org.org_name.clone(),
            description: clotho_with_org.repo.description.clone(),
            visibility: clotho_with_org.repo.visibility.clone(),
            default_branch: clotho_with_org.repo.default_branch.clone(),
            clone_url,
            provider,
            configured,
            info,
            main_commit_id: String::new(),
            heads: vec![],
        }))
    } else {
        Err(ApiError::Internal(
            "database is not configured; repo settings require the control plane".into(),
        ))
    }
}

pub async fn delete_repo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("database is not configured".into()))?;
    let clotho = control::require_repo_admin(pool, &name, &auth.user_id).await?;
    let git_owner = clotho.repo.forgejo_owner.clone();
    let repo_id = clotho.repo.id.clone();
    let org_id = clotho.repo.org_id.clone();

    control::delete_repo_row(pool, &repo_id).await?;

    if let Err(e) = state.forgejo.delete_repo(&git_owner, &name).await {
        tracing::warn!(repo = %name, error = %e, "best-effort collab provider repo delete failed");
    }

    control::log_activity(
        pool,
        ActivityEventInput {
            actor_id: auth.user_id,
            org_id: Some(org_id),
            repo_id: None,
            event_type: "repo.deleted".into(),
            payload: serde_json::json!({"repo_name": name}),
        },
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
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
    /// Base64 bytes when the materialized file is not UTF-8.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_base64: Option<String>,
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
    let bytes = crate::arachne::materialize_pointer(&state, &file.content)
        .await?
        .unwrap_or(file.content);
    let size_bytes = bytes.len() as u64;
    let content = String::from_utf8(bytes.clone()).ok();
    let content_base64 = content
        .is_none()
        .then(|| base64::engine::general_purpose::STANDARD.encode(bytes));
    Ok(Json(FileResponse {
        commit_id: file.commit_id,
        path: file.path,
        executable: file.executable,
        conflicted: file.conflicted,
        size_bytes,
        binary: content.is_none(),
        content,
        content_base64,
    }))
}

#[derive(Serialize)]
pub struct ArachneFileJson {
    pub path: String,
    pub logical_bytes: u64,
    pub pointer_bytes: u64,
    pub oid_sha256: String,
    pub arachne_hash: String,
}

#[derive(Serialize)]
pub struct RepoStorageStatsResponse {
    pub commit_id: String,
    pub git_tree_bytes: u64,
    pub logical_bytes: u64,
    pub arachne_file_count: u64,
    pub arachne_logical_bytes: u64,
    pub large_files: Vec<ArachneFileJson>,
    /// Physical metrics are store-scoped until per-org buckets land.
    pub store_scope: String,
    pub xorb_count: u64,
    pub xorb_bytes: u64,
    pub shard_count: u64,
    pub shard_bytes: u64,
    pub store_total_bytes: u64,
}

/// Canonical repository storage view: logical payload sizes from Arachne
/// pointers plus honest physical metrics for the active managed store.
pub async fn storage_stats(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<RepoStorageStatsResponse>, ApiError> {
    let mut vcs = state.vcs.clone();
    let tree = vcs
        .list_files(ListFilesRequest {
            repo: name.clone(),
            commit_id: String::new(),
        })
        .await?
        .into_inner();
    let git_tree_bytes: u64 = tree.files.iter().map(|entry| entry.size_bytes).sum();
    let mut logical_bytes = git_tree_bytes;
    let mut arachne_logical_bytes = 0u64;
    let mut large_files = Vec::new();
    for entry in tree.files {
        // Clotho pointers are tiny. Avoid reading ordinary large git blobs.
        if entry.size_bytes > 1024 {
            continue;
        }
        let file = vcs
            .get_file(GetFileRequest {
                repo: name.clone(),
                commit_id: tree.commit_id.clone(),
                path: entry.path.clone(),
            })
            .await?
            .into_inner();
        let Ok(pointer) = clotho_common::lfs_pointer::LfsPointer::parse(&file.content) else {
            continue;
        };
        logical_bytes = logical_bytes
            .saturating_sub(entry.size_bytes)
            .saturating_add(pointer.size);
        arachne_logical_bytes = arachne_logical_bytes.saturating_add(pointer.size);
        large_files.push(ArachneFileJson {
            path: entry.path,
            logical_bytes: pointer.size,
            pointer_bytes: entry.size_bytes,
            oid_sha256: pointer.oid_sha256,
            arachne_hash: pointer.arachne_hash,
        });
    }
    let store = state
        .storage
        .clone()
        .get_storage_stats(GetStorageStatsRequest {})
        .await?
        .into_inner();
    Ok(Json(RepoStorageStatsResponse {
        commit_id: tree.commit_id,
        git_tree_bytes,
        logical_bytes,
        arachne_file_count: large_files.len() as u64,
        arachne_logical_bytes,
        large_files,
        store_scope: "managed-default".into(),
        xorb_count: store.xorb_count,
        xorb_bytes: store.xorb_bytes,
        shard_count: store.shard_count,
        shard_bytes: store.shard_bytes,
        store_total_bytes: store.total_bytes,
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
