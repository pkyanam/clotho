//! Repo-browser read endpoints: repo list/detail, tree, file contents,
//! commit log, and the jj operation log — clotho-vcs aggregated into the
//! JSON the web app renders (ADR-0007).
//!
//! Stage 11 makes the repo list/detail query Clotho's control-plane tables
//! first, then overlays collaboration metadata. Slice A adds PATCH/DELETE for
//! repo settings.

use std::collections::{BTreeMap, HashMap};
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
    pub kind: String,
    pub large_file_threshold_bytes: i64,
    pub network_mode: String,
    pub network_tags: Vec<String>,
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
    let kind = repo_info.kind.clone();
    let large_file_threshold_bytes = repo_info.large_file_threshold_bytes;
    let network_mode = repo_info.network_mode.clone();
    let network_tags = repo_info.network_tags.clone();
    Ok(Json(RepoDetailResponse {
        name,
        owner,
        owner_org,
        description,
        visibility,
        kind,
        large_file_threshold_bytes,
        network_mode,
        network_tags,
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
                    "kind": req.kind,
                    "large_file_threshold_bytes": req.large_file_threshold_bytes,
                    "network_mode": req.network_mode,
                    "network_tags": req.network_tags,
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
            kind: clotho_with_org.repo.kind.clone(),
            large_file_threshold_bytes: clotho_with_org.repo.large_file_threshold_bytes,
            network_mode: clotho_with_org.repo.network_mode.clone(),
            network_tags: clotho_with_org.repo.network_tags.clone(),
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

#[derive(Serialize)]
pub struct ArtifactEntryJson {
    pub path: String,
    /// Semantic purpose in an ML repository, such as weights, dataset_shard,
    /// tokenizer, card, evaluation, or source.
    pub role: String,
    /// Portable on-disk format inferred from the filename.
    pub format: String,
    /// Broad workload family used by clients for grouping.
    pub family: String,
    /// Logical bytes after composing an Arachne pointer.
    pub size_bytes: u64,
    pub storage: String,
    pub conflicted: bool,
}

#[derive(Serialize)]
pub struct ArtifactReadinessJson {
    pub card: bool,
    pub primary_artifacts: bool,
    pub metadata: bool,
    pub ready: bool,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct ArtifactManifestResponse {
    pub commit_id: String,
    pub kind: String,
    pub total_files: u64,
    pub total_bytes: u64,
    pub arachne_files: u64,
    pub role_counts: BTreeMap<String, u64>,
    pub format_counts: BTreeMap<String, u64>,
    pub readiness: ArtifactReadinessJson,
    pub artifacts: Vec<ArtifactEntryJson>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ArtifactClass {
    role: &'static str,
    format: &'static str,
    family: &'static str,
}

fn artifact_class(path: &str) -> ArtifactClass {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let extension = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");

    if matches!(
        name,
        "readme" | "readme.md" | "modelcard.md" | "datasetcard.md"
    ) {
        return ArtifactClass {
            role: "card",
            format: "markdown",
            family: "documentation",
        };
    }
    if name.contains("tokenizer") || matches!(name, "vocab.json" | "merges.txt" | "spiece.model") {
        return ArtifactClass {
            role: "tokenizer",
            format: match extension {
                "model" => "sentencepiece",
                "txt" => "text",
                _ => "json",
            },
            family: "model",
        };
    }
    if name.contains("eval") || name.contains("metric") || name.contains("benchmark") {
        return ArtifactClass {
            role: "evaluation",
            format: structured_format(extension),
            family: "evaluation",
        };
    }
    if matches!(
        name,
        "config.json" | "generation_config.json" | "model_index.json"
    ) {
        return ArtifactClass {
            role: "model_config",
            format: "json",
            family: "model",
        };
    }

    let model_format = match extension {
        "safetensors" => Some("safetensors"),
        "gguf" => Some("gguf"),
        "onnx" => Some("onnx"),
        "pt" | "pth" | "bin" => Some("pytorch"),
        "ckpt" => Some("checkpoint"),
        "h5" | "keras" | "pb" => Some("tensorflow"),
        "tflite" => Some("tflite"),
        "mlmodel" | "mlpackage" => Some("coreml"),
        _ => None,
    };
    if let Some(format) = model_format {
        return ArtifactClass {
            role: "weights",
            format,
            family: "model",
        };
    }

    let dataset_format = match extension {
        "parquet" => Some("parquet"),
        "arrow" => Some("arrow"),
        "jsonl" | "ndjson" => Some("jsonl"),
        "csv" => Some("csv"),
        "tsv" => Some("tsv"),
        "avro" => Some("avro"),
        "orc" => Some("orc"),
        _ => None,
    };
    if let Some(format) = dataset_format {
        return ArtifactClass {
            role: "dataset_shard",
            format,
            family: "dataset",
        };
    }
    if matches!(
        name,
        "dataset_info.json" | "dataset_infos.json" | "features.json"
    ) {
        return ArtifactClass {
            role: "dataset_schema",
            format: "json",
            family: "dataset",
        };
    }
    if matches!(extension, "md" | "mdx" | "rst") {
        return ArtifactClass {
            role: "documentation",
            format: if extension == "rst" {
                "rst"
            } else {
                "markdown"
            },
            family: "documentation",
        };
    }
    if matches!(extension, "json" | "yaml" | "yml" | "toml") {
        return ArtifactClass {
            role: "metadata",
            format: structured_format(extension),
            family: "metadata",
        };
    }
    if matches!(
        extension,
        "py" | "rs" | "ts" | "tsx" | "js" | "jsx" | "go" | "sh"
    ) {
        return ArtifactClass {
            role: "source",
            format: extension_format(extension),
            family: "source",
        };
    }
    ArtifactClass {
        role: "other",
        format: extension_format(extension),
        family: "other",
    }
}

fn structured_format(extension: &str) -> &'static str {
    match extension {
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "csv" => "csv",
        _ => "json",
    }
}

fn extension_format(extension: &str) -> &'static str {
    match extension {
        "py" => "python",
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "go" => "go",
        "sh" => "shell",
        "txt" => "text",
        "png" | "jpg" | "jpeg" | "webp" => "image",
        "wav" | "flac" | "mp3" => "audio",
        "mp4" | "webm" => "video",
        "tar" | "gz" | "zip" => "archive",
        "" => "unknown",
        _ => "other",
    }
}

/// Semantic inventory for code, model, and dataset repositories. Clotho owns
/// this view: clients do not need to download multi-GB payloads or inspect the
/// backing Forgejo repository to understand what a repository contains.
pub async fn artifact_manifest(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<TreeQuery>,
) -> Result<Json<ArtifactManifestResponse>, ApiError> {
    let (repo, ..) = load_repo_detail(&state, &name).await?;
    let kind = repo.kind;
    let mut vcs = state.vcs.clone();
    let tree = vcs
        .list_files(ListFilesRequest {
            repo: name.clone(),
            commit_id: query.commit_id,
        })
        .await?
        .into_inner();

    let mut artifacts = Vec::with_capacity(tree.files.len());
    let mut total_bytes = 0u64;
    let mut arachne_files = 0u64;
    let mut role_counts = BTreeMap::new();
    let mut format_counts = BTreeMap::new();
    for entry in tree.files {
        let mut logical_bytes = entry.size_bytes;
        let mut storage = "git";
        if entry.size_bytes <= 1024 {
            let file = vcs
                .get_file(GetFileRequest {
                    repo: name.clone(),
                    commit_id: tree.commit_id.clone(),
                    path: entry.path.clone(),
                })
                .await?
                .into_inner();
            if let Ok(pointer) = clotho_common::lfs_pointer::LfsPointer::parse(&file.content) {
                logical_bytes = pointer.size;
                storage = "arachne";
                arachne_files += 1;
            }
        }
        let class = artifact_class(&entry.path);
        *role_counts.entry(class.role.to_string()).or_insert(0) += 1;
        *format_counts.entry(class.format.to_string()).or_insert(0) += 1;
        total_bytes = total_bytes.saturating_add(logical_bytes);
        artifacts.push(ArtifactEntryJson {
            path: entry.path,
            role: class.role.into(),
            format: class.format.into(),
            family: class.family.into(),
            size_bytes: logical_bytes,
            storage: storage.into(),
            conflicted: entry.conflicted,
        });
    }
    artifacts.sort_by(|a, b| {
        b.size_bytes
            .cmp(&a.size_bytes)
            .then_with(|| a.path.cmp(&b.path))
    });

    let card = role_counts.contains_key("card");
    let metadata = role_counts.keys().any(|role| {
        matches!(
            role.as_str(),
            "model_config" | "dataset_schema" | "metadata"
        )
    });
    let primary_role = if kind == "model" {
        "weights"
    } else if kind == "dataset" {
        "dataset_shard"
    } else {
        "source"
    };
    let primary_artifacts = role_counts.contains_key(primary_role);
    let mut warnings = Vec::new();
    if matches!(kind.as_str(), "model" | "dataset") && !card {
        warnings.push(format!("add a README.md {} card", kind));
    }
    if !primary_artifacts {
        warnings.push(format!("no {primary_role} artifacts detected"));
    }
    if matches!(kind.as_str(), "model" | "dataset") && !metadata {
        warnings.push(format!("add structured {kind} metadata"));
    }
    let ready = if matches!(kind.as_str(), "model" | "dataset") {
        card && primary_artifacts && metadata
    } else {
        primary_artifacts
    };

    Ok(Json(ArtifactManifestResponse {
        commit_id: tree.commit_id,
        kind,
        total_files: artifacts.len() as u64,
        total_bytes,
        arachne_files,
        role_counts,
        format_counts,
        readiness: ArtifactReadinessJson {
            card,
            primary_artifacts,
            metadata,
            ready,
            warnings,
        },
        artifacts,
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

#[cfg(test)]
mod artifact_tests {
    use super::*;

    #[test]
    fn classifies_portable_model_artifacts() {
        assert_eq!(
            artifact_class("model-00001-of-00004.safetensors").role,
            "weights"
        );
        assert_eq!(artifact_class("weights/model.Q4_K_M.gguf").format, "gguf");
        assert_eq!(artifact_class("tokenizer.json").role, "tokenizer");
        assert_eq!(artifact_class("config.json").role, "model_config");
        assert_eq!(artifact_class("README.md").role, "card");
    }

    #[test]
    fn classifies_dataset_and_evaluation_artifacts() {
        let shard = artifact_class("data/train-00001-of-00010.parquet");
        assert_eq!(shard.role, "dataset_shard");
        assert_eq!(shard.family, "dataset");
        assert_eq!(artifact_class("dataset_info.json").role, "dataset_schema");
        assert_eq!(
            artifact_class("benchmarks/eval_results.json").role,
            "evaluation"
        );
    }
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
