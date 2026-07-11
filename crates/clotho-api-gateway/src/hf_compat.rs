//! Read-only Hugging Face Hub compatibility over Clotho-owned releases.
//! Standard `huggingface_hub.HfApi(endpoint=...)` clients can inspect and
//! download models/datasets without making Forgejo or Hugging Face canonical.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Response};
use axum::Json;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::auth;
use crate::control;
use crate::error::ApiError;
use crate::releases::{self, ReleaseSnapshot};
use crate::AppState;

#[derive(Deserialize, Default)]
pub struct InfoQuery {
    pub revision: Option<String>,
    #[serde(default, alias = "blobs")]
    pub files_metadata: bool,
}

#[derive(Deserialize, Default)]
pub struct TreeCompatQuery {
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub expand: bool,
}

#[derive(Deserialize, Default)]
pub struct ListCompatQuery {
    pub search: Option<String>,
    pub filter: Option<String>,
    pub author: Option<String>,
    pub pipeline_tag: Option<String>,
    pub gated: Option<bool>,
    pub limit: Option<usize>,
    #[serde(default)]
    pub full: bool,
    #[serde(default, rename = "cardData")]
    pub card_data: bool,
}

#[derive(Deserialize, Default)]
pub struct RefsCompatQuery {
    pub include_prs: Option<u8>,
}

async fn snapshot(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    name: &str,
    revision: &str,
    kind: &str,
) -> Result<(control::RepoWithOrg, ReleaseSnapshot), ApiError> {
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("Hub compatibility requires the control plane".into()))?;
    let repo = control::get_repo_with_org(pool, name)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("repository {owner}/{name} not found")))?;
    if repo.org_name != owner && repo.repo.forgejo_owner != owner {
        return Err(ApiError::NotFound(format!(
            "repository {owner}/{name} not found"
        )));
    }
    if repo.repo.kind != kind {
        return Err(ApiError::NotFound(format!(
            "{kind} repository {owner}/{name} not found"
        )));
    }
    authorize_hub_read(state, headers, &repo).await?;
    let release = releases::resolve_release_snapshot(pool, &repo.repo.id, revision).await?;
    Ok((repo, release))
}

async fn authorize_hub_read(
    state: &AppState,
    headers: &HeaderMap,
    repo: &control::RepoWithOrg,
) -> Result<(), ApiError> {
    let explicit_token = auth::extract_bearer(headers).is_some();
    if repo.repo.visibility == "public" {
        // Anonymous public reads are intentional. If a caller did send a
        // credential, still validate it so a bad token is never ignored.
        if explicit_token {
            auth::resolve_auth(headers, state).await?;
        }
        return Ok(());
    }
    let auth = auth::resolve_auth(headers, state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("Hub compatibility requires the control plane".into()))?;
    control::require_repo_permission(pool, &repo.repo.name, &auth.user_id, "read").await?;
    Ok(())
}

pub async fn model_info(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<InfoQuery>,
) -> Result<Json<Value>, ApiError> {
    repo_info(state, headers, owner, name, query, "model").await
}

pub async fn dataset_info(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<InfoQuery>,
) -> Result<Json<Value>, ApiError> {
    repo_info(state, headers, owner, name, query, "dataset").await
}

pub async fn list_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListCompatQuery>,
) -> Result<Json<Vec<Value>>, ApiError> {
    list_hub_repos(state, headers, query, "model").await
}

pub async fn list_datasets(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListCompatQuery>,
) -> Result<Json<Vec<Value>>, ApiError> {
    list_hub_repos(state, headers, query, "dataset").await
}

pub async fn model_refs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<RefsCompatQuery>,
) -> Result<Json<Value>, ApiError> {
    repo_refs(state, headers, owner, name, query, "model").await
}

pub async fn dataset_refs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<RefsCompatQuery>,
) -> Result<Json<Value>, ApiError> {
    repo_refs(state, headers, owner, name, query, "dataset").await
}

async fn repo_refs(
    state: Arc<AppState>,
    headers: HeaderMap,
    owner: String,
    name: String,
    query: RefsCompatQuery,
    kind: &str,
) -> Result<Json<Value>, ApiError> {
    let (repo, latest) = snapshot(&state, &headers, &owner, &name, "main", kind).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("Hub compatibility requires the control plane".into()))?;
    let versions = sqlx::query_scalar::<_, String>(
        "select version from repo_releases where repo_id = $1 order by created_at desc",
    )
    .bind(&repo.repo.id)
    .fetch_all(pool)
    .await
    .map_err(|error| ApiError::Internal(format!("list Hub release refs: {error}")))?;
    let mut tags = Vec::new();
    for version in versions {
        match releases::resolve_release_snapshot(pool, &repo.repo.id, &version).await {
            Ok(release) => tags.push(json!({
                "name": version,
                "ref": format!("refs/tags/{}", release.version),
                "targetCommit": release.commit_id,
            })),
            Err(ApiError::Conflict(_) | ApiError::NotFound(_)) => continue,
            Err(error) => return Err(error),
        }
    }
    let mut value = json!({
        "branches": [{
            "name": "main",
            "ref": "refs/heads/main",
            "targetCommit": latest.commit_id,
        }],
        "converts": [],
        "tags": tags,
    });
    if query.include_prs.unwrap_or(0) > 0 {
        value["pullRequests"] = json!([]);
    }
    Ok(Json(value))
}

async fn list_hub_repos(
    state: Arc<AppState>,
    headers: HeaderMap,
    query: ListCompatQuery,
    kind: &str,
) -> Result<Json<Vec<Value>>, ApiError> {
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("Hub compatibility requires the control plane".into()))?;
    let candidates = control::list_repos_with_orgs(pool).await?;
    let auth = if auth::extract_bearer(&headers).is_some() || !state.auth_required {
        Some(auth::resolve_auth(&headers, &state).await?)
    } else {
        None
    };
    let search = query.search.as_deref().map(str::to_ascii_lowercase);
    let filter = query.filter.as_deref().map(str::to_ascii_lowercase);
    let author = query.author.as_deref().map(str::to_ascii_lowercase);
    let pipeline_tag = query.pipeline_tag.as_deref().map(str::to_ascii_lowercase);
    let limit = query.limit.unwrap_or(usize::MAX);
    if limit == 0 {
        return Ok(Json(Vec::new()));
    }
    let mut results = Vec::new();
    for candidate in candidates {
        if query.gated == Some(true)
            || candidate.repo.kind != kind
            || author
                .as_ref()
                .is_some_and(|author| candidate.org_name.to_ascii_lowercase() != *author)
        {
            continue;
        }
        if candidate.repo.visibility != "public" {
            let Some(auth) = &auth else {
                continue;
            };
            match control::require_repo_permission(
                pool,
                &candidate.repo.name,
                &auth.user_id,
                "read",
            )
            .await
            {
                Ok(_) => {}
                Err(ApiError::Forbidden(_) | ApiError::NotFound(_)) => continue,
                Err(error) => return Err(error),
            }
        }
        let release =
            match releases::resolve_release_snapshot(pool, &candidate.repo.id, "main").await {
                Ok(release) => release,
                Err(ApiError::NotFound(_) | ApiError::Conflict(_)) => continue,
                Err(error) => return Err(error),
            };
        let searchable = format!(
            "{}/{} {} {}",
            candidate.org_name,
            candidate.repo.name,
            candidate.repo.description,
            release.manifest["metadata"]
        )
        .to_ascii_lowercase();
        if search
            .as_ref()
            .is_some_and(|needle| !searchable.contains(needle))
        {
            continue;
        }
        if filter
            .as_ref()
            .is_some_and(|needle| !searchable.contains(needle))
            || pipeline_tag.as_ref().is_some_and(|tag| {
                release.manifest["metadata"]["pipeline_tag"]
                    .as_str()
                    .map(str::to_ascii_lowercase)
                    .as_deref()
                    != Some(tag.as_str())
            })
        {
            continue;
        }
        results.push(repo_info_value(
            &candidate,
            release,
            query.full || query.card_data,
        ));
        if results.len() == limit {
            break;
        }
    }
    Ok(Json(results))
}

pub async fn model_info_revision(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, name, revision)): Path<(String, String, String)>,
    Query(mut query): Query<InfoQuery>,
) -> Result<Json<Value>, ApiError> {
    query.revision = Some(revision);
    repo_info(state, headers, owner, name, query, "model").await
}

pub async fn dataset_info_revision(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, name, revision)): Path<(String, String, String)>,
    Query(mut query): Query<InfoQuery>,
) -> Result<Json<Value>, ApiError> {
    query.revision = Some(revision);
    repo_info(state, headers, owner, name, query, "dataset").await
}

async fn repo_info(
    state: Arc<AppState>,
    headers: HeaderMap,
    owner: String,
    name: String,
    query: InfoQuery,
    kind: &str,
) -> Result<Json<Value>, ApiError> {
    let revision = query.revision.as_deref().unwrap_or("main");
    let (repo, release) = snapshot(&state, &headers, &owner, &name, revision, kind).await?;
    Ok(Json(repo_info_value(&repo, release, query.files_metadata)))
}

fn repo_info_value(
    repo: &control::RepoWithOrg,
    release: ReleaseSnapshot,
    files_metadata: bool,
) -> Value {
    let artifacts = release.manifest["artifacts"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let siblings = artifacts
        .iter()
        .map(|artifact| sibling(artifact, &release.commit_id, files_metadata))
        .collect::<Vec<_>>();
    let metadata = release.manifest["metadata"].clone();
    let tags = discovery_tags(&metadata, &repo.repo.kind);
    let id = format!("{}/{}", repo.org_name, repo.repo.name);
    json!({
        "id": id,
        "modelId": id,
        "author": repo.org_name,
        "sha": release.commit_id,
        "lastModified": hub_datetime(release.created_at),
        "private": repo.repo.visibility != "public",
        "gated": false,
        "disabled": false,
        "downloads": 0,
        "likes": 0,
        "tags": tags,
        "pipeline_tag": metadata.get("pipeline_tag").and_then(Value::as_str),
        "library_name": metadata.get("library_name").and_then(Value::as_str),
        "cardData": metadata,
        "siblings": siblings,
        "usedStorage": release.manifest["total_bytes"],
        "clotho": {
            "release": release.version,
            "manifest_sha256": release.manifest_sha256,
            "evaluation_count": metadata.get("evaluations").and_then(Value::as_array).map_or(0, Vec::len),
            "source_of_truth": true
        }
    })
}

fn sibling(artifact: &Value, commit_id: &str, files_metadata: bool) -> Value {
    let path = artifact["path"].as_str().unwrap_or_default();
    let size = artifact["size_bytes"].as_u64().unwrap_or(0);
    let oid = artifact["oid_sha256"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| synthetic_oid(commit_id, path));
    let mut value = json!({"rfilename": path});
    if files_metadata {
        value["size"] = json!(size);
        value["blobId"] = json!(oid);
        if artifact["storage"] == "arachne" {
            value["lfs"] = json!({"size": size, "sha256": oid, "pointerSize": 0});
        }
    }
    value
}

fn discovery_tags(metadata: &Value, kind: &str) -> Vec<String> {
    let mut tags = vec![kind.to_string(), "clotho".into(), "arachne".into()];
    for key in ["pipeline_tag", "library_name", "license"] {
        if let Some(value) = metadata.get(key).and_then(Value::as_str) {
            tags.push(value.to_string());
        }
    }
    if let Some(values) = metadata.get("tags").and_then(Value::as_array) {
        tags.extend(values.iter().filter_map(Value::as_str).map(str::to_string));
    }
    tags.sort();
    tags.dedup();
    tags
}

pub async fn model_tree(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, name, revision)): Path<(String, String, String)>,
    Query(query): Query<TreeCompatQuery>,
) -> Result<Response<Body>, ApiError> {
    tree(state, headers, owner, name, revision, query, "model").await
}

pub async fn dataset_tree(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, name, revision)): Path<(String, String, String)>,
    Query(query): Query<TreeCompatQuery>,
) -> Result<Response<Body>, ApiError> {
    tree(state, headers, owner, name, revision, query, "dataset").await
}

async fn tree(
    state: Arc<AppState>,
    headers: HeaderMap,
    owner: String,
    name: String,
    revision: String,
    query: TreeCompatQuery,
    kind: &str,
) -> Result<Response<Body>, ApiError> {
    let (_, release) = snapshot(&state, &headers, &owner, &name, &revision, kind).await?;
    let artifacts = release.manifest["artifacts"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let entries = tree_entries(
        &artifacts,
        &release.commit_id,
        query.recursive,
        query.expand,
    );
    let body = serde_json::to_vec(&entries)
        .map_err(|error| ApiError::Internal(format!("encode Hub tree: {error}")))?;
    let mut response = Response::new(Body::from(body));
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response.headers_mut().insert(
        axum::http::header::HeaderName::from_static("x-repo-commit"),
        HeaderValue::from_str(&release.commit_id)
            .map_err(|error| ApiError::Internal(format!("Hub tree header: {error}")))?,
    );
    Ok(response)
}

fn tree_entries(artifacts: &[Value], commit_id: &str, recursive: bool, expand: bool) -> Vec<Value> {
    let mut folders = BTreeMap::<String, Value>::new();
    let mut files = Vec::new();
    for artifact in artifacts {
        let path = artifact["path"].as_str().unwrap_or_default();
        if !recursive {
            if let Some((folder, _)) = path.split_once('/') {
                folders.entry(folder.into()).or_insert_with(|| {
                    json!({"type":"directory", "oid":synthetic_oid(commit_id, folder), "path":folder})
                });
                continue;
            }
        }
        let size = artifact["size_bytes"].as_u64().unwrap_or(0);
        let oid = artifact["oid_sha256"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| synthetic_oid(commit_id, path));
        let mut file = json!({"type":"file", "oid":oid, "size":size, "path":path});
        if artifact["storage"] == "arachne" {
            file["lfs"] = json!({"size":size, "oid":oid, "pointerSize":0});
        }
        if expand {
            file["securityFileStatus"] = json!({"status":"safe"});
        }
        files.push(file);
    }
    folders.into_values().chain(files).collect()
}

fn synthetic_oid(commit_id: &str, path: &str) -> String {
    format!("{:x}", Sha256::digest(format!("{commit_id}\0{path}")))
}

fn hub_datetime(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

pub async fn model_resolve_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, name, revision, path)): Path<(String, String, String, String)>,
) -> Result<Response<Body>, ApiError> {
    resolve(
        state,
        headers,
        CompatResolve {
            owner,
            name,
            revision,
            path,
            kind: "model",
            head_only: false,
        },
    )
    .await
}

pub async fn model_resolve_head(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, name, revision, path)): Path<(String, String, String, String)>,
) -> Result<Response<Body>, ApiError> {
    resolve(
        state,
        headers,
        CompatResolve {
            owner,
            name,
            revision,
            path,
            kind: "model",
            head_only: true,
        },
    )
    .await
}

pub async fn dataset_resolve_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, name, revision, path)): Path<(String, String, String, String)>,
) -> Result<Response<Body>, ApiError> {
    resolve(
        state,
        headers,
        CompatResolve {
            owner,
            name,
            revision,
            path,
            kind: "dataset",
            head_only: false,
        },
    )
    .await
}

pub async fn dataset_resolve_head(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, name, revision, path)): Path<(String, String, String, String)>,
) -> Result<Response<Body>, ApiError> {
    resolve(
        state,
        headers,
        CompatResolve {
            owner,
            name,
            revision,
            path,
            kind: "dataset",
            head_only: true,
        },
    )
    .await
}

struct CompatResolve {
    owner: String,
    name: String,
    revision: String,
    path: String,
    kind: &'static str,
    head_only: bool,
}

async fn resolve(
    state: Arc<AppState>,
    headers: HeaderMap,
    target: CompatResolve,
) -> Result<Response<Body>, ApiError> {
    let (_, release) = snapshot(
        &state,
        &headers,
        &target.owner,
        &target.name,
        &target.revision,
        target.kind,
    )
    .await?;
    releases::serve_release_file(
        state,
        headers,
        target.name,
        release.version,
        target.path,
        target.head_only,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_recursive_tree_groups_folders() {
        let artifacts = vec![
            json!({"path":"README.md","size_bytes":5,"storage":"git"}),
            json!({"path":"weights/model.bin","size_bytes":9,"storage":"arachne"}),
            json!({"path":"weights/config.json","size_bytes":2,"storage":"git"}),
        ];
        let root = tree_entries(&artifacts, "abc", false, false);
        assert_eq!(root.len(), 2);
        assert!(root.iter().any(|entry| entry["type"] == "directory"));
        let recursive = tree_entries(&artifacts, "abc", true, true);
        assert_eq!(recursive.len(), 3);
        assert_eq!(recursive[1]["securityFileStatus"]["status"], "safe");
        assert!(recursive[1]["lfs"].get("oid").is_some());
        assert!(recursive[1]["lfs"].get("sha256").is_none());
    }

    #[test]
    fn hub_datetimes_use_the_clients_strict_utc_shape() {
        let value = DateTime::parse_from_rfc3339("2026-07-11T07:35:46.949277+00:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(hub_datetime(value), "2026-07-11T07:35:46.949277Z");
    }
}
