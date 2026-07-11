//! Immutable, Clotho-owned releases for code, model, and dataset repositories.
//! A release binds a human version to a Git commit and a canonical semantic
//! artifact manifest. Forgejo remains an implementation detail.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
}
