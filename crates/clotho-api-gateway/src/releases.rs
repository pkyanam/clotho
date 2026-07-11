//! Immutable, Clotho-owned releases for code, model, and dataset repositories.
//! A release binds a human version to a Git commit and a canonical semantic
//! artifact manifest. Forgejo remains an implementation detail.

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::header::{
    ACCEPT_RANGES, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG, RANGE,
};
use axum::http::{HeaderMap, HeaderValue, Response, StatusCode};
use axum::Json;
use chrono::{DateTime, Utc};
use clotho_common::lfs_pointer::{LfsPointer, PointerError};
use clotho_common::pb::storage::v1::DownloadFileRequest;
use clotho_common::pb::vcs::v1::GetFileRequest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use crate::auth;
use crate::control::{self, ActivityEventInput};
use crate::error::ApiError;
use crate::repos::{self, TreeQuery};
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateReleaseRequest {
    pub version: String,
    #[serde(default)]
    pub commit_id: String,
    #[serde(default = "default_require_ready")]
    pub require_ready: bool,
}

fn default_require_ready() -> bool {
    true
}

#[derive(sqlx::FromRow)]
struct DbRelease {
    id: String,
    version: String,
    commit_id: String,
    manifest: serde_json::Value,
    manifest_sha256: String,
    created_by: String,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct ReleaseSummary {
    pub id: String,
    pub version: String,
    pub commit_id: String,
    pub manifest_sha256: String,
    pub kind: String,
    pub total_files: u64,
    pub total_bytes: u64,
    pub ready: bool,
    pub verified: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct ReleaseRecord {
    #[serde(flatten)]
    pub summary: ReleaseSummary,
    pub manifest: serde_json::Value,
}

#[derive(Serialize)]
pub struct ReleaseList {
    pub releases: Vec<ReleaseSummary>,
}

fn validate_version(version: &str) -> Result<(), ApiError> {
    if version.is_empty()
        || version.len() > 100
        || version.starts_with('.')
        || version.ends_with('.')
        || !version
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '.' | '-' | '_' | '+'))
    {
        return Err(ApiError::InvalidRequest(
            "release version must be 1-100 characters using letters, numbers, ., -, _, or +".into(),
        ));
    }
    Ok(())
}

fn manifest_digest(manifest: &serde_json::Value) -> Result<String, ApiError> {
    let canonical = serde_json::to_vec(manifest)
        .map_err(|err| ApiError::Internal(format!("serialize release manifest: {err}")))?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

fn summary(row: &DbRelease) -> Result<ReleaseSummary, ApiError> {
    Ok(ReleaseSummary {
        id: row.id.clone(),
        version: row.version.clone(),
        commit_id: row.commit_id.clone(),
        manifest_sha256: row.manifest_sha256.clone(),
        kind: row.manifest["kind"].as_str().unwrap_or("code").into(),
        total_files: row.manifest["total_files"].as_u64().unwrap_or(0),
        total_bytes: row.manifest["total_bytes"].as_u64().unwrap_or(0),
        ready: row.manifest["readiness"]["ready"]
            .as_bool()
            .unwrap_or(false),
        verified: manifest_digest(&row.manifest)? == row.manifest_sha256,
        created_by: row.created_by.clone(),
        created_at: row.created_at,
    })
}

async fn load_release(
    pool: &sqlx::PgPool,
    repo_id: &str,
    version: &str,
) -> Result<DbRelease, ApiError> {
    sqlx::query_as::<_, DbRelease>(
        r#"select id, version, commit_id, manifest, manifest_sha256,
                  created_by, created_at
           from repo_releases where repo_id = $1 and version = $2"#,
    )
    .bind(repo_id)
    .bind(version)
    .fetch_optional(pool)
    .await
    .map_err(|err| ApiError::Internal(format!("load release: {err}")))?
    .ok_or_else(|| ApiError::NotFound(format!("release {version:?} not found")))
}

pub(crate) struct ReleaseBinding {
    pub commit_id: String,
    pub manifest_sha256: String,
    pub ready: bool,
}

pub(crate) struct ReleaseSnapshot {
    pub version: String,
    pub commit_id: String,
    pub manifest_sha256: String,
    pub manifest: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub(crate) async fn resolve_release_snapshot(
    pool: &sqlx::PgPool,
    repo_id: &str,
    revision: &str,
) -> Result<ReleaseSnapshot, ApiError> {
    let row = if revision == "main" || revision.is_empty() {
        sqlx::query_as::<_, DbRelease>(
            r#"select id, version, commit_id, manifest, manifest_sha256,
                      created_by, created_at from repo_releases
               where repo_id = $1 order by created_at desc limit 1"#,
        )
        .bind(repo_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| ApiError::Internal(format!("resolve latest release: {error}")))?
        .ok_or_else(|| ApiError::NotFound("repository has no immutable releases".into()))?
    } else {
        sqlx::query_as::<_, DbRelease>(
            r#"select id, version, commit_id, manifest, manifest_sha256,
                      created_by, created_at from repo_releases
               where repo_id = $1 and (version = $2 or commit_id = $2)
               order by created_at desc limit 1"#,
        )
        .bind(repo_id)
        .bind(revision)
        .fetch_optional(pool)
        .await
        .map_err(|error| ApiError::Internal(format!("resolve release revision: {error}")))?
        .ok_or_else(|| ApiError::NotFound(format!("release revision {revision:?} not found")))?
    };
    if !summary(&row)?.verified {
        return Err(ApiError::Conflict(format!(
            "release {:?} failed manifest verification",
            row.version
        )));
    }
    Ok(ReleaseSnapshot {
        version: row.version,
        commit_id: row.commit_id,
        manifest_sha256: row.manifest_sha256,
        manifest: row.manifest,
        created_at: row.created_at,
    })
}

pub(crate) async fn release_binding(
    pool: &sqlx::PgPool,
    repo_id: &str,
    version: &str,
) -> Result<ReleaseBinding, ApiError> {
    let row = load_release(pool, repo_id, version).await?;
    let release_summary = summary(&row)?;
    if !release_summary.verified {
        return Err(ApiError::Conflict(format!(
            "release {version:?} failed manifest verification"
        )));
    }
    Ok(ReleaseBinding {
        commit_id: row.commit_id,
        manifest_sha256: row.manifest_sha256,
        ready: release_summary.ready,
    })
}

pub async fn create_release(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(request): Json<CreateReleaseRequest>,
) -> Result<(StatusCode, Json<ReleaseRecord>), ApiError> {
    validate_version(&request.version)?;
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("releases require the control plane".into()))?;
    let repo = control::require_repo_permission(pool, &name, &auth.user_id, "write").await?;
    let manifest = repos::artifact_manifest(
        State(state.clone()),
        Path(name.clone()),
        Query(TreeQuery {
            commit_id: request.commit_id,
        }),
    )
    .await?
    .0;
    if request.require_ready && !manifest.readiness.ready {
        return Err(ApiError::Conflict(format!(
            "repository is not release-ready: {}",
            manifest.readiness.warnings.join("; ")
        )));
    }
    let commit_id = manifest.commit_id.clone();
    let manifest = serde_json::to_value(manifest)
        .map_err(|err| ApiError::Internal(format!("build release manifest: {err}")))?;
    let digest = manifest_digest(&manifest)?;
    let id = Uuid::new_v4().to_string();
    let inserted = sqlx::query_as::<_, DbRelease>(
        r#"insert into repo_releases
           (id, repo_id, version, commit_id, manifest, manifest_sha256, created_by)
           values ($1, $2, $3, $4, $5, $6, $7)
           returning id, version, commit_id, manifest, manifest_sha256,
                     created_by, created_at"#,
    )
    .bind(&id)
    .bind(&repo.repo.id)
    .bind(&request.version)
    .bind(&commit_id)
    .bind(&manifest)
    .bind(&digest)
    .bind(&auth.user_id)
    .fetch_one(pool)
    .await;
    let row = match inserted {
        Ok(row) => row,
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            return Err(ApiError::Conflict(format!(
                "release {:?} already exists and releases are immutable",
                request.version
            )));
        }
        Err(err) => return Err(ApiError::Internal(format!("create release: {err}"))),
    };
    control::log_activity(
        pool,
        ActivityEventInput {
            actor_id: auth.user_id,
            org_id: Some(repo.repo.org_id),
            repo_id: Some(repo.repo.id),
            event_type: "repo.release_created".into(),
            payload: serde_json::json!({
                "repo_name": name,
                "version": row.version,
                "commit_id": row.commit_id,
                "manifest_sha256": row.manifest_sha256,
            }),
        },
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(ReleaseRecord {
            summary: summary(&row)?,
            manifest: row.manifest,
        }),
    ))
}

pub async fn list_releases(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<ReleaseList>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("releases require the control plane".into()))?;
    let repo = control::require_repo_permission(pool, &name, &auth.user_id, "read").await?;
    let rows = sqlx::query_as::<_, DbRelease>(
        r#"select id, version, commit_id, manifest, manifest_sha256,
                  created_by, created_at from repo_releases
           where repo_id = $1 order by created_at desc limit 100"#,
    )
    .bind(repo.repo.id)
    .fetch_all(pool)
    .await
    .map_err(|err| ApiError::Internal(format!("list releases: {err}")))?;
    let releases = rows.iter().map(summary).collect::<Result<Vec<_>, _>>()?;
    Ok(Json(ReleaseList { releases }))
}

pub async fn get_release(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, version)): Path<(String, String)>,
) -> Result<Json<ReleaseRecord>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("releases require the control plane".into()))?;
    let repo = control::require_repo_permission(pool, &name, &auth.user_id, "read").await?;
    let row = load_release(pool, &repo.repo.id, &version).await?;
    Ok(Json(ReleaseRecord {
        summary: summary(&row)?,
        manifest: row.manifest,
    }))
}

pub async fn get_release_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, version, path)): Path<(String, String, String)>,
) -> Result<Response<Body>, ApiError> {
    serve_release_file(state, headers, name, version, path, false).await
}

pub async fn head_release_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, version, path)): Path<(String, String, String)>,
) -> Result<Response<Body>, ApiError> {
    serve_release_file(state, headers, name, version, path, true).await
}

pub(crate) async fn serve_release_file(
    state: Arc<AppState>,
    headers: HeaderMap,
    name: String,
    version: String,
    path: String,
    head_only: bool,
) -> Result<Response<Body>, ApiError> {
    crate::validate_commit_path(&path)?;
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("release downloads require the control plane".into()))?;
    let repo = control::require_repo_permission(pool, &name, &auth.user_id, "read").await?;
    let release = load_release(pool, &repo.repo.id, &version).await?;
    let release_summary = summary(&release)?;
    if !release_summary.verified {
        return Err(ApiError::Conflict(format!(
            "release {version:?} failed manifest verification"
        )));
    }
    let in_manifest = release.manifest["artifacts"]
        .as_array()
        .is_some_and(|artifacts| artifacts.iter().any(|artifact| artifact["path"] == path));
    if !in_manifest {
        return Err(ApiError::NotFound(format!(
            "file {path:?} is not part of release {version:?}"
        )));
    }

    let file = state
        .vcs
        .clone()
        .get_file(GetFileRequest {
            repo: name,
            commit_id: release.commit_id.clone(),
            path: path.clone(),
        })
        .await?
        .into_inner();
    if file.conflicted {
        return Err(ApiError::Conflict(
            "conflicted files cannot be served from an immutable release".into(),
        ));
    }

    let (body, response_size, total_size, byte_range, oid, arachne_hash) =
        match LfsPointer::parse(&file.content) {
            Ok(pointer) => {
                let byte_range = parse_byte_range(&headers, pointer.size)?;
                let (offset, length) = byte_range
                    .map(|range| (range.start, range.len()))
                    .unwrap_or((0, pointer.size));
                let body = if head_only {
                    Body::empty()
                } else {
                    stream_arachne_release(&state, pointer.clone(), offset, length).await?
                };
                (
                    body,
                    length,
                    pointer.size,
                    byte_range,
                    pointer.oid_sha256,
                    pointer.arachne_hash,
                )
            }
            Err(PointerError::NotPointer) => {
                let oid = format!("{:x}", Sha256::digest(&file.content));
                let total_size = file.content.len() as u64;
                let byte_range = parse_byte_range(&headers, total_size)?;
                let (start, length) = byte_range
                    .map(|range| (range.start, range.len()))
                    .unwrap_or((0, total_size));
                let body = if head_only {
                    Body::empty()
                } else {
                    let start = usize::try_from(start).map_err(|_| {
                        ApiError::Internal("release range exceeds host limits".into())
                    })?;
                    let end = usize::try_from(start as u64 + length).map_err(|_| {
                        ApiError::Internal("release range exceeds host limits".into())
                    })?;
                    Body::from(file.content[start..end].to_vec())
                };
                (body, length, total_size, byte_range, oid, String::new())
            }
            Err(error) => return Err(ApiError::Upstream(error.to_string())),
        };
    let mut response = Response::new(body);
    if let Some(range) = byte_range {
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
        insert_header(
            response.headers_mut(),
            CONTENT_RANGE,
            &format!("bytes {}-{}/{}", range.start, range.end, total_size),
        )?;
    }
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type(&path)));
    response
        .headers_mut()
        .insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    insert_header(
        response.headers_mut(),
        CONTENT_LENGTH,
        &response_size.to_string(),
    )?;
    insert_header(response.headers_mut(), ETAG, &format!("\"sha256:{oid}\""))?;
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static(if repo.repo.visibility == "public" {
            "public, max-age=31536000, immutable"
        } else {
            "private, max-age=31536000, immutable"
        }),
    );
    insert_named_header(&mut response, "x-clotho-release-version", &version)?;
    insert_named_header(&mut response, "x-clotho-commit-id", &release.commit_id)?;
    // Keep the native provenance headers while also speaking the standard Hub
    // download metadata protocol. This lets unmodified huggingface_hub clients
    // cache files by Clotho's immutable release commit.
    insert_named_header(&mut response, "x-repo-commit", &release.commit_id)?;
    insert_named_header(&mut response, "x-linked-etag", &format!("\"{oid}\""))?;
    insert_named_header(&mut response, "x-linked-size", &total_size.to_string())?;
    insert_named_header(
        &mut response,
        "x-clotho-manifest-sha256",
        &release.manifest_sha256,
    )?;
    if !arachne_hash.is_empty() {
        insert_named_header(&mut response, "x-clotho-arachne-hash", &arachne_hash)?;
    }
    Ok(response)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    fn len(self) -> u64 {
        self.end - self.start + 1
    }
}

fn parse_byte_range(headers: &HeaderMap, size: u64) -> Result<Option<ByteRange>, ApiError> {
    let Some(raw) = headers.get(RANGE) else {
        return Ok(None);
    };
    let raw = raw.to_str().map_err(|_| ApiError::RangeNotSatisfiable {
        message: "Range must contain visible ASCII".into(),
        size,
    })?;
    let Some(spec) = raw.strip_prefix("bytes=") else {
        return Err(range_error(size));
    };
    if spec.contains(',') || size == 0 {
        return Err(range_error(size));
    }
    let Some((start, end)) = spec.split_once('-') else {
        return Err(range_error(size));
    };
    let range = if start.is_empty() {
        let suffix = end.parse::<u64>().ok().filter(|value| *value > 0);
        let Some(suffix) = suffix else {
            return Err(range_error(size));
        };
        ByteRange {
            start: size.saturating_sub(suffix.min(size)),
            end: size - 1,
        }
    } else {
        let Some(start) = start.parse::<u64>().ok().filter(|start| *start < size) else {
            return Err(range_error(size));
        };
        let end = if end.is_empty() {
            size - 1
        } else {
            let Some(end) = end.parse::<u64>().ok().filter(|end| *end >= start) else {
                return Err(range_error(size));
            };
            end.min(size - 1)
        };
        ByteRange { start, end }
    };
    Ok(Some(range))
}

fn range_error(size: u64) -> ApiError {
    ApiError::RangeNotSatisfiable {
        message: "only one satisfiable byte range is supported".into(),
        size,
    }
}

async fn stream_arachne_release(
    state: &AppState,
    pointer: LfsPointer,
    offset: u64,
    length: u64,
) -> Result<Body, ApiError> {
    let mut stream = state
        .storage
        .clone()
        .download_file(DownloadFileRequest {
            file_hash: pointer.arachne_hash.clone(),
            offset,
            length,
        })
        .await?
        .into_inner();
    let (sender, receiver) = mpsc::channel::<Result<Bytes, std::io::Error>>(4);
    tokio::spawn(async move {
        let verify_full_payload = offset == 0 && length == pointer.size;
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        while let Ok(Some(block)) = stream.message().await {
            received = received.saturating_add(block.data.len() as u64);
            if verify_full_payload {
                hasher.update(&block.data);
            }
            if sender.send(Ok(Bytes::from(block.data))).await.is_err() {
                return;
            }
        }
        let digest = format!("{:x}", hasher.finalize());
        if received != length || (verify_full_payload && digest != pointer.oid_sha256) {
            let _ = sender
                .send(Err(std::io::Error::other(format!(
                    "Arachne release integrity failure: expected {length}/{}, received {received}/{digest}",
                    pointer.oid_sha256
                ))))
                .await;
        }
    });
    Ok(Body::from_stream(ReceiverStream::new(receiver)))
}

fn insert_header(
    headers: &mut HeaderMap,
    name: axum::http::header::HeaderName,
    value: &str,
) -> Result<(), ApiError> {
    headers.insert(
        name,
        HeaderValue::from_str(value)
            .map_err(|error| ApiError::Internal(format!("release response header: {error}")))?,
    );
    Ok(())
}

fn insert_named_header(
    response: &mut Response<Body>,
    name: &'static str,
    value: &str,
) -> Result<(), ApiError> {
    insert_header(
        response.headers_mut(),
        axum::http::header::HeaderName::from_static(name),
        value,
    )
}

fn content_type(path: &str) -> &'static str {
    match path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
    {
        Some(extension) if extension == "json" || extension == "jsonl" => "application/json",
        Some(extension) if extension == "md" || extension == "txt" => "text/plain; charset=utf-8",
        Some(extension) if extension == "csv" => "text/csv; charset=utf-8",
        Some(extension) if extension == "safetensors" => "application/x-safetensors",
        Some(extension) if extension == "onnx" => "application/onnx",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_path_safe_and_human_readable() {
        for valid in ["v1.0.0", "2026-07-11", "model_7+cuda"] {
            assert!(validate_version(valid).is_ok());
        }
        for invalid in ["", ".hidden", "v1/latest", "v1 latest", "v1?"] {
            assert!(validate_version(invalid).is_err());
        }
    }

    #[test]
    fn manifests_are_tamper_evident() {
        let one = serde_json::json!({"commit_id":"abc","artifacts":[]});
        let two = serde_json::json!({"commit_id":"def","artifacts":[]});
        assert_eq!(manifest_digest(&one).unwrap().len(), 64);
        assert_ne!(
            manifest_digest(&one).unwrap(),
            manifest_digest(&two).unwrap()
        );
    }

    #[test]
    fn release_downloads_use_portable_content_types() {
        assert_eq!(
            content_type("weights/model.safetensors"),
            "application/x-safetensors"
        );
        assert_eq!(content_type("README.md"), "text/plain; charset=utf-8");
        assert_eq!(
            content_type("weights/model.bin"),
            "application/octet-stream"
        );
    }

    #[test]
    fn release_downloads_parse_standard_single_ranges() {
        let headers = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(RANGE, HeaderValue::from_str(value).unwrap());
            headers
        };
        assert_eq!(
            parse_byte_range(&headers("bytes=10-19"), 100).unwrap(),
            Some(ByteRange { start: 10, end: 19 })
        );
        assert_eq!(
            parse_byte_range(&headers("bytes=90-"), 100).unwrap(),
            Some(ByteRange { start: 90, end: 99 })
        );
        assert_eq!(
            parse_byte_range(&headers("bytes=-10"), 100).unwrap(),
            Some(ByteRange { start: 90, end: 99 })
        );
        assert!(parse_byte_range(&headers("bytes=100-101"), 100).is_err());
        assert!(parse_byte_range(&headers("bytes=0-1,4-5"), 100).is_err());
    }
}
