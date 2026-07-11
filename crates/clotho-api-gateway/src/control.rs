//! Clotho control plane (docs/prd.md §5 Stage 11): users, orgs, repos,
//! memberships, permissions, and activity events.
//!
//! Auth is intentionally a Stage 11 placeholder: a deterministic bootstrap
//! user/org is created from env/defaults. Real human auth and encrypted
//! secrets are explicitly deferred.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::ApiError;
use crate::forgejo::RepoInfo;
use crate::AppState;

/// Deterministic bootstrap identity for the Stage 11 placeholder auth model.
#[derive(Clone, Debug)]
pub struct Bootstrap {
    pub user_id: String,
    pub user_name: String,
    pub user_email: String,
    pub org_id: String,
    pub org_name: String,
    pub org_display_name: String,
    pub forgejo_owner: String,
}

impl Bootstrap {
    pub fn from_config(config: &crate::GatewayConfig) -> Self {
        Self {
            user_id: slug(&config.bootstrap_user_name),
            user_name: config.bootstrap_user_name.clone(),
            user_email: config.bootstrap_user_email.clone(),
            org_id: slug(&config.bootstrap_org_name),
            org_name: config.bootstrap_org_name.clone(),
            org_display_name: config.bootstrap_org_display_name.clone(),
            forgejo_owner: config.forgejo.owner.clone(),
        }
    }
}

fn slug(s: &str) -> String {
    s.trim().to_lowercase().replace(' ', "-").replace('"', "")
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: String,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct Org {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub forgejo_owner: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

/// Public org shape — never exposes internal git owner field names.
#[derive(Clone, Debug, Serialize)]
pub struct OrgPublic {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

impl From<Org> for OrgPublic {
    fn from(o: Org) -> Self {
        Self {
            id: o.id,
            name: o.name,
            display_name: o.display_name,
            created_by: o.created_by,
            created_at: o.created_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct OrgMembership {
    pub org_id: String,
    pub user_id: String,
    pub role: String,
    pub user_name: String,
    pub user_display_name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct OrgWithMembers {
    pub org: Org,
    pub members: Vec<OrgMembership>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct Repo {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub description: String,
    pub visibility: String,
    pub kind: String,
    pub large_file_threshold_bytes: i64,
    pub network_mode: String,
    pub network_tags: Vec<String>,
    pub default_branch: String,
    pub forgejo_owner: String,
    pub forgejo_repo_id: Option<i64>,
    pub forgejo_full_name: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct RepoWithOrg {
    #[sqlx(flatten)]
    pub repo: Repo,
    pub org_name: String,
    pub org_display_name: String,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
#[allow(dead_code)]
pub struct RepoPermission {
    pub repo_id: String,
    pub user_id: String,
    pub permission: String,
    pub user_name: String,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct ActivityEvent {
    pub id: i64,
    pub actor_id: String,
    pub org_id: Option<String>,
    pub repo_id: Option<String>,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActivityEventInput {
    pub actor_id: String,
    pub org_id: Option<String>,
    pub repo_id: Option<String>,
    pub event_type: String,
    pub payload: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct UserListResponse {
    pub users: Vec<User>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OrgListResponse {
    pub orgs: Vec<OrgPublic>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OrgDetailResponse {
    pub org: OrgPublic,
    pub members: Vec<OrgMembership>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActivityListResponse {
    pub events: Vec<ActivityEvent>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateOrgRequest {
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default, alias = "forgejo_owner")]
    pub git_owner: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateRepoRequest {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub large_file_threshold_bytes: Option<i64>,
    #[serde(default)]
    pub network_mode: Option<String>,
    #[serde(default)]
    pub network_tags: Option<Vec<String>>,
    #[serde(default)]
    pub default_branch: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateRepoRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_visibility")]
    pub visibility: String,
    #[serde(default = "default_repo_kind")]
    pub kind: String,
    /// Omit to use the kind-aware default: 10 MiB for code, 1 MiB for model
    /// and dataset repos. Zero routes every non-empty payload through Arachne.
    #[serde(default)]
    pub large_file_threshold_bytes: Option<i64>,
    #[serde(default = "default_network_mode")]
    pub network_mode: String,
    #[serde(default)]
    pub network_tags: Vec<String>,
    #[serde(default = "default_default_branch")]
    pub default_branch: String,
    #[serde(default)]
    pub owner_org: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ActivityQuery {
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

pub const DEFAULT_ACTIVITY_PAGE_SIZE: usize = 50;
pub const MAX_ACTIVITY_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ActivityCursor {
    version: u8,
    created_at: DateTime<Utc>,
    id: i64,
}

fn default_visibility() -> String {
    "public".into()
}

fn default_repo_kind() -> String {
    "code".into()
}

fn default_network_mode() -> String {
    "public".into()
}

pub fn validate_network_policy(mode: &str, tags: &[String]) -> Result<(), ApiError> {
    if !matches!(mode, "public" | "tailscale") {
        return Err(ApiError::InvalidRequest(format!(
            "network_mode {mode:?} must be public or tailscale"
        )));
    }
    if mode == "tailscale" && tags.is_empty() {
        return Err(ApiError::InvalidRequest(
            "tailscale network mode requires at least one tag".into(),
        ));
    }
    if tags.iter().any(|tag| {
        !tag.starts_with("tag:")
            || tag.len() > 128
            || !tag
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_'))
    }) {
        return Err(ApiError::InvalidRequest(
            "network tags must use tag:<name> with letters, digits, '-', or '_'".into(),
        ));
    }
    Ok(())
}

pub fn validate_repo_kind(kind: &str) -> Result<(), ApiError> {
    if matches!(kind, "code" | "model" | "dataset") {
        Ok(())
    } else {
        Err(ApiError::InvalidRequest(format!(
            "repository kind {kind:?} must be code, model, or dataset"
        )))
    }
}

pub fn effective_large_file_threshold(req: &CreateRepoRequest) -> Result<i64, ApiError> {
    validate_repo_kind(&req.kind)?;
    let threshold = req.large_file_threshold_bytes.unwrap_or_else(|| {
        if req.kind == "code" {
            10 * 1024 * 1024
        } else {
            1024 * 1024
        }
    });
    if threshold < 0 {
        return Err(ApiError::InvalidRequest(
            "large_file_threshold_bytes cannot be negative".into(),
        ));
    }
    Ok(threshold)
}

fn default_default_branch() -> String {
    "main".into()
}

pub fn valid_name(name: &str) -> Result<(), ApiError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ApiError::InvalidRequest("name is required".into()));
    }
    if name.len() > 128 {
        return Err(ApiError::InvalidRequest("name is too long".into()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(ApiError::InvalidRequest(
            "name must be lowercase letters, digits, '-' or '_'".into(),
        ));
    }
    Ok(())
}

fn is_conflict(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.constraint())
        .map(|c| c.contains("unique") || c.contains("_key") || c.contains("_pkey"))
        .unwrap_or(false)
        || e.to_string().to_lowercase().contains("unique constraint")
        || e.to_string().to_lowercase().contains("already exists")
}

/// Ensure the deterministic bootstrap user and org exist in Postgres.
pub async fn ensure_bootstrap(pool: &PgPool, b: &Bootstrap) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into users (id, name, email, display_name)
        values ($1, $2, $3, $4)
        on conflict (id) do nothing
        "#,
    )
    .bind(&b.user_id)
    .bind(&b.user_name)
    .bind(&b.user_email)
    .bind(&b.user_name)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("bootstrap user insert: {e}")))?;

    sqlx::query(
        r#"
        insert into orgs (id, name, display_name, forgejo_owner, created_by)
        values ($1, $2, $3, $4, $5)
        on conflict (id) do nothing
        "#,
    )
    .bind(&b.org_id)
    .bind(&b.org_name)
    .bind(&b.org_display_name)
    .bind(&b.forgejo_owner)
    .bind(&b.user_id)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("bootstrap org insert: {e}")))?;

    sqlx::query(
        r#"
        insert into org_memberships (org_id, user_id, role)
        values ($1, $2, 'admin')
        on conflict (org_id, user_id) do nothing
        "#,
    )
    .bind(&b.org_id)
    .bind(&b.user_id)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("bootstrap membership insert: {e}")))?;

    crate::auth::ensure_bootstrap_token(pool, &b.user_id).await?;

    Ok(())
}

pub async fn list_users(pool: &PgPool) -> Result<Vec<User>, ApiError> {
    sqlx::query_as::<_, User>("select * from users order by created_at desc")
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("list users: {e}")))
}

pub async fn list_orgs(pool: &PgPool) -> Result<Vec<Org>, ApiError> {
    sqlx::query_as::<_, Org>("select * from orgs order by created_at desc")
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("list orgs: {e}")))
}

pub async fn create_org(
    pool: &PgPool,
    actor_id: &str,
    req: &CreateOrgRequest,
    default_git_owner: &str,
) -> Result<Org, ApiError> {
    valid_name(&req.name)?;
    let name = req.name.trim().to_lowercase();
    let display_name = if req.display_name.is_empty() {
        name.clone()
    } else {
        req.display_name.trim().into()
    };
    let forgejo_owner = if req.git_owner.is_empty() {
        default_git_owner.to_string()
    } else {
        req.git_owner.trim().into()
    };
    let id = slug(&name);

    let row = sqlx::query(
        r#"
        insert into orgs (id, name, display_name, forgejo_owner, created_by)
        values ($1, $2, $3, $4, $5)
        returning id, name, display_name, forgejo_owner, created_by, created_at
        "#,
    )
    .bind(&id)
    .bind(&name)
    .bind(&display_name)
    .bind(&forgejo_owner)
    .bind(actor_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if is_conflict(&e) {
            ApiError::Conflict(format!("org {name:?} already exists"))
        } else {
            ApiError::Internal(format!("create org: {e}"))
        }
    })?;

    sqlx::query("insert into org_memberships (org_id, user_id, role) values ($1, $2, 'admin')")
        .bind(&id)
        .bind(actor_id)
        .execute(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("create org membership: {e}")))?;

    log_activity(
        pool,
        ActivityEventInput {
            actor_id: actor_id.to_string(),
            org_id: Some(id.clone()),
            repo_id: None,
            event_type: "org.created".into(),
            payload: serde_json::json!({"org_name": name}),
        },
    )
    .await?;

    Ok(Org {
        id: row.get("id"),
        name: row.get("name"),
        display_name: row.get("display_name"),
        forgejo_owner: row.get("forgejo_owner"),
        created_by: row.get("created_by"),
        created_at: row.get::<DateTime<Utc>, _>("created_at"),
    })
}

/// Fetch an org by name or id; 404 if missing.
pub async fn get_org(pool: &PgPool, org: &str) -> Result<Org, ApiError> {
    sqlx::query_as::<_, Org>("select * from orgs where name = $1 or id = $1")
        .bind(org)
        .fetch_optional(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("get org: {e}")))?
        .ok_or_else(|| ApiError::NotFound(format!("org {org:?} not found")))
}

/// Fetch a Clotho repo by unique name; 404 if missing.
pub async fn get_repo_by_name(pool: &PgPool, name: &str) -> Result<Repo, ApiError> {
    let row = get_repo_with_org(pool, name).await?;
    row.map(|r| r.repo)
        .ok_or_else(|| ApiError::NotFound(format!("repo {name:?} not found")))
}

/// Name-routed repositories must be globally unambiguous until Stage 23 adds
/// tenant-qualified public paths. Reject an existing name before any VCS or
/// collaboration-provider side effect.
pub async fn require_global_repo_name_available(pool: &PgPool, name: &str) -> Result<(), ApiError> {
    let count: i64 = sqlx::query_scalar("select count(*)::bigint from repos where name = $1")
        .bind(name)
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("check repository name: {e}")))?;
    if count > 0 {
        return Err(ApiError::Conflict(format!(
            "repository name {name:?} is already in use"
        )));
    }
    Ok(())
}

pub async fn get_org_with_members(
    pool: &PgPool,
    org: &str,
) -> Result<Option<OrgWithMembers>, ApiError> {
    let Some(org) = sqlx::query_as::<_, Org>("select * from orgs where name = $1 or id = $1")
        .bind(org)
        .fetch_optional(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("get org: {e}")))?
    else {
        return Ok(None);
    };

    let members = sqlx::query_as::<_, OrgMembership>(
        r#"
        select m.org_id, m.user_id, m.role, u.name as user_name, u.display_name as user_display_name
        from org_memberships m
        join users u on u.id = m.user_id
        where m.org_id = $1
        "#,
    )
    .bind(&org.id)
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("get org members: {e}")))?;

    Ok(Some(OrgWithMembers { org, members }))
}

pub async fn get_repo_with_org(pool: &PgPool, name: &str) -> Result<Option<RepoWithOrg>, ApiError> {
    let rows = sqlx::query_as::<_, RepoWithOrg>(
        r#"
        select
            r.id, r.org_id, r.name, r.description, r.visibility, r.kind,
            r.large_file_threshold_bytes,
            r.network_mode, r.network_tags,
            r.default_branch, r.forgejo_owner, r.forgejo_repo_id,
            r.forgejo_full_name, r.created_by, r.created_at, r.updated_at,
            o.name as org_name, o.display_name as org_display_name
        from repos r
        join orgs o on o.id = r.org_id
        where r.name = $1
        order by r.updated_at desc
        limit 2
        "#,
    )
    .bind(name)
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("get repo: {e}")))?;
    if rows.len() > 1 {
        tracing::warn!(repo = %name, "ambiguous name-routed repository lookup denied");
        return Ok(None);
    }
    Ok(rows.into_iter().next())
}

const ACCESSIBLE_REPOS_SQL: &str = r#"
        select
            r.id, r.org_id, r.name, r.description, r.visibility, r.kind,
            r.large_file_threshold_bytes,
            r.network_mode, r.network_tags,
            r.default_branch, r.forgejo_owner, r.forgejo_repo_id,
            r.forgejo_full_name, r.created_by, r.created_at, r.updated_at,
            o.name as org_name, o.display_name as org_display_name
        from repos r
        join orgs o on o.id = r.org_id
        where true
          and not exists (
            select 1 from repos duplicate
            where duplicate.name = r.name and duplicate.id <> r.id
          )
          and (
            r.visibility = 'public'
            or (
              $1::text is not null and (
                exists (
                  select 1 from org_memberships membership
                  where membership.org_id = r.org_id
                    and membership.user_id = $1
                    and membership.role = 'admin'
                )
                or exists (
                  select 1 from repo_permissions permission
                  where permission.repo_id = r.id
                    and permission.user_id = $1
                    and permission.permission in ('read', 'write', 'admin')
                )
              )
            )
          )
        order by r.updated_at desc
        "#;

const ACCESSIBLE_ORG_REPOS_SQL: &str = r#"
        select
            r.id, r.org_id, r.name, r.description, r.visibility, r.kind,
            r.large_file_threshold_bytes,
            r.network_mode, r.network_tags,
            r.default_branch, r.forgejo_owner, r.forgejo_repo_id,
            r.forgejo_full_name, r.created_by, r.created_at, r.updated_at,
            o.name as org_name, o.display_name as org_display_name
        from repos r
        join orgs o on o.id = r.org_id
        where (o.name = $2 or o.id = $2)
          and not exists (
            select 1 from repos duplicate
            where duplicate.name = r.name and duplicate.id <> r.id
          )
          and (
            r.visibility = 'public'
            or (
              $1::text is not null and (
                exists (
                  select 1 from org_memberships membership
                  where membership.org_id = r.org_id
                    and membership.user_id = $1
                    and membership.role = 'admin'
                )
                or exists (
                  select 1 from repo_permissions permission
                  where permission.repo_id = r.id
                    and permission.user_id = $1
                    and permission.permission in ('read', 'write', 'admin')
                )
              )
            )
          )
        order by r.updated_at desc
        "#;

/// Filter repository visibility and permission in Postgres before pagination
/// or provider overlay. Ambiguous global names are excluded fail-closed.
pub async fn list_accessible_repos_with_orgs(
    pool: &PgPool,
    user_id: Option<&str>,
) -> Result<Vec<RepoWithOrg>, ApiError> {
    sqlx::query_as::<_, RepoWithOrg>(ACCESSIBLE_REPOS_SQL)
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("list accessible repos: {e}")))
}

pub async fn list_accessible_repos_for_org(
    pool: &PgPool,
    org: &str,
    user_id: Option<&str>,
) -> Result<Vec<RepoWithOrg>, ApiError> {
    sqlx::query_as::<_, RepoWithOrg>(ACCESSIBLE_ORG_REPOS_SQL)
        .bind(user_id)
        .bind(org)
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("list accessible org repos: {e}")))
}

pub async fn list_repos_with_orgs(pool: &PgPool) -> Result<Vec<RepoWithOrg>, ApiError> {
    sqlx::query_as::<_, RepoWithOrg>(
        r#"
        select
            r.id, r.org_id, r.name, r.description, r.visibility, r.kind,
            r.large_file_threshold_bytes,
            r.network_mode, r.network_tags,
            r.default_branch, r.forgejo_owner, r.forgejo_repo_id,
            r.forgejo_full_name, r.created_by, r.created_at, r.updated_at,
            o.name as org_name, o.display_name as org_display_name
        from repos r
        join orgs o on o.id = r.org_id
        order by r.updated_at desc
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("list repos: {e}")))
}

pub async fn list_repos_for_org(pool: &PgPool, org: &str) -> Result<Vec<RepoWithOrg>, ApiError> {
    sqlx::query_as::<_, RepoWithOrg>(
        r#"
        select
            r.id, r.org_id, r.name, r.description, r.visibility, r.kind,
            r.large_file_threshold_bytes,
            r.network_mode, r.network_tags,
            r.default_branch, r.forgejo_owner, r.forgejo_repo_id,
            r.forgejo_full_name, r.created_by, r.created_at, r.updated_at,
            o.name as org_name, o.display_name as org_display_name
        from repos r
        join orgs o on o.id = r.org_id
        where o.name = $1 or o.id = $1
        order by r.updated_at desc
        "#,
    )
    .bind(org)
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("list org repos: {e}")))
}

/// Resolve a requested org name/id to its id, display name, and Forgejo owner.
pub async fn resolve_org(
    pool: &PgPool,
    b: &Bootstrap,
    org_name_or_id: &str,
) -> Result<(String, String, String), ApiError> {
    if org_name_or_id.is_empty() {
        return Ok((
            b.org_id.clone(),
            b.org_name.clone(),
            b.forgejo_owner.clone(),
        ));
    }
    let row = sqlx::query("select id, name, forgejo_owner from orgs where name = $1 or id = $1")
        .bind(org_name_or_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("resolve org: {e}")))?;
    match row {
        Some(row) => Ok((
            row.get::<String, _>("id"),
            row.get::<String, _>("name"),
            row.get::<String, _>("forgejo_owner"),
        )),
        None => Err(ApiError::NotFound(format!(
            "org {org_name_or_id:?} not found"
        ))),
    }
}

pub async fn insert_repo(
    pool: &PgPool,
    actor_id: &str,
    req: &CreateRepoRequest,
    (org_id, org_name, forgejo_owner): &(String, String, String),
    forgejo_repo: &RepoInfo,
) -> Result<RepoWithOrg, ApiError> {
    valid_name(&req.name)?;
    let large_file_threshold_bytes = effective_large_file_threshold(req)?;
    validate_network_policy(&req.network_mode, &req.network_tags)?;
    let id = Uuid::new_v4().to_string();
    let full_name = format!("{forgejo_owner}/{}", req.name);

    let row = sqlx::query(
        r#"
        insert into repos (
            id, org_id, name, description, visibility, kind,
            large_file_threshold_bytes, default_branch,
            network_mode, network_tags, forgejo_owner, forgejo_repo_id,
            forgejo_full_name, created_by
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        returning id, org_id, name, description, visibility, kind,
                large_file_threshold_bytes, default_branch,
                network_mode, network_tags,
                forgejo_owner, forgejo_repo_id, forgejo_full_name,
                created_by, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(org_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.visibility)
    .bind(&req.kind)
    .bind(large_file_threshold_bytes)
    .bind(&req.default_branch)
    .bind(&req.network_mode)
    .bind(&req.network_tags)
    .bind(forgejo_owner)
    .bind(forgejo_repo.id)
    .bind(&full_name)
    .bind(actor_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if is_conflict(&e) {
            ApiError::Conflict(format!(
                "repo {:?} already exists in {org_name:?}",
                req.name
            ))
        } else {
            ApiError::Internal(format!("insert repo: {e}"))
        }
    })?;

    let repo = Repo {
        id: row.get("id"),
        org_id: row.get("org_id"),
        name: row.get("name"),
        description: row.get("description"),
        visibility: row.get("visibility"),
        kind: row.get("kind"),
        large_file_threshold_bytes: row.get("large_file_threshold_bytes"),
        network_mode: row.get("network_mode"),
        network_tags: row.get("network_tags"),
        default_branch: row.get("default_branch"),
        forgejo_owner: row.get("forgejo_owner"),
        forgejo_repo_id: row.get("forgejo_repo_id"),
        forgejo_full_name: row.get("forgejo_full_name"),
        created_by: row.get("created_by"),
        created_at: row.get::<DateTime<Utc>, _>("created_at"),
        updated_at: row.get::<DateTime<Utc>, _>("updated_at"),
    };

    sqlx::query(
        "insert into repo_permissions (repo_id, user_id, permission) values ($1, $2, 'admin')",
    )
    .bind(&repo.id)
    .bind(actor_id)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("insert repo permissions: {e}")))?;

    log_activity(
        pool,
        ActivityEventInput {
            actor_id: actor_id.to_string(),
            org_id: Some(org_id.clone()),
            repo_id: Some(repo.id.clone()),
            event_type: "repo.created".into(),
            payload: serde_json::json!({
                "repo_name": req.name,
                "org_name": org_name,
                "visibility": req.visibility,
                "kind": req.kind,
                "large_file_threshold_bytes": large_file_threshold_bytes,
                "network_mode": req.network_mode,
                "network_tags": req.network_tags,
                "default_branch": req.default_branch,
            }),
        },
    )
    .await?;

    Ok(RepoWithOrg {
        repo,
        org_name: org_name.clone(),
        org_display_name: org_name.clone(),
    })
}

pub async fn log_activity(pool: &PgPool, event: ActivityEventInput) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into activity_events (actor_id, org_id, repo_id, event_type, payload)
        values ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(&event.actor_id)
    .bind(&event.org_id)
    .bind(&event.repo_id)
    .bind(&event.event_type)
    .bind(&event.payload)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("log activity: {e}")))?;
    Ok(())
}

async fn list_activity(
    pool: &PgPool,
    limit: i64,
    cursor: Option<&ActivityCursor>,
    user_id: Option<&str>,
) -> Result<Vec<ActivityEvent>, ApiError> {
    let query = if let Some(cursor) = cursor {
        sqlx::query_as::<_, ActivityEvent>(
            r#"
            select e.* from activity_events e
            where (e.created_at < $2 or (e.created_at = $2 and e.id < $3))
              and (
                (e.repo_id is null and e.org_id is null)
                or (
                  e.repo_id is null and $4::text is not null and exists (
                    select 1 from org_memberships m
                    where m.org_id = e.org_id and m.user_id = $4
                  )
                )
                or exists (
                  select 1 from repos r
                  where r.id = e.repo_id
                    and not exists (
                      select 1 from repos duplicate
                      where duplicate.name = r.name and duplicate.id <> r.id
                    )
                    and (
                      r.visibility = 'public'
                      or ($4::text is not null and (
                        exists (
                          select 1 from org_memberships m
                          where m.org_id = r.org_id and m.user_id = $4 and m.role = 'admin'
                        )
                        or exists (
                          select 1 from repo_permissions p
                          where p.repo_id = r.id and p.user_id = $4
                            and p.permission in ('read', 'write', 'admin')
                        )
                      ))
                    )
                )
              )
            order by e.created_at desc, e.id desc
            limit $1
            "#,
        )
        .bind(limit)
        .bind(cursor.created_at)
        .bind(cursor.id)
        .bind(user_id)
    } else {
        sqlx::query_as::<_, ActivityEvent>(
            r#"
            select e.* from activity_events e
            where (
              (e.repo_id is null and e.org_id is null)
              or (
                e.repo_id is null and $2::text is not null and exists (
                  select 1 from org_memberships m
                  where m.org_id = e.org_id and m.user_id = $2
                )
              )
              or exists (
                select 1 from repos r
                where r.id = e.repo_id
                  and not exists (
                    select 1 from repos duplicate
                    where duplicate.name = r.name and duplicate.id <> r.id
                  )
                  and (
                    r.visibility = 'public'
                    or ($2::text is not null and (
                      exists (
                        select 1 from org_memberships m
                        where m.org_id = r.org_id and m.user_id = $2 and m.role = 'admin'
                      )
                      or exists (
                        select 1 from repo_permissions p
                        where p.repo_id = r.id and p.user_id = $2
                          and p.permission in ('read', 'write', 'admin')
                      )
                    ))
                  )
              )
            )
            order by e.created_at desc, e.id desc
            limit $1
            "#,
        )
        .bind(limit)
        .bind(user_id)
    };
    query
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("list activity: {e}")))
}

/// Agent activity is restricted to repository-scoped events authorized by the
/// token's repository scope. Filtering happens in SQL before the limit/cursor;
/// org-only and global human events are intentionally absent because current
/// agent tokens have no organization scope.
async fn list_activity_for_agent(
    pool: &PgPool,
    limit: i64,
    cursor: Option<&ActivityCursor>,
    allowed_repos: &[String],
) -> Result<Vec<ActivityEvent>, ApiError> {
    let wildcard = allowed_repos.iter().any(|repo| repo == "*");
    let query = if let Some(cursor) = cursor {
        sqlx::query_as::<_, ActivityEvent>(
            r#"
            select e.* from activity_events e
            where (e.created_at < $2 or (e.created_at = $2 and e.id < $3))
              and e.repo_id is not null
              and exists (
                select 1 from repos r
                where r.id = e.repo_id
                  and not exists (
                    select 1 from repos duplicate
                    where duplicate.name = r.name and duplicate.id <> r.id
                  )
                  and ($4 or r.name = any($5))
              )
            order by e.created_at desc, e.id desc
            limit $1
            "#,
        )
        .bind(limit)
        .bind(cursor.created_at)
        .bind(cursor.id)
        .bind(wildcard)
        .bind(allowed_repos)
    } else {
        sqlx::query_as::<_, ActivityEvent>(
            r#"
            select e.* from activity_events e
            where e.repo_id is not null
              and exists (
                select 1 from repos r
                where r.id = e.repo_id
                  and not exists (
                    select 1 from repos duplicate
                    where duplicate.name = r.name and duplicate.id <> r.id
                  )
                  and ($2 or r.name = any($3))
              )
            order by e.created_at desc, e.id desc
            limit $1
            "#,
        )
        .bind(limit)
        .bind(wildcard)
        .bind(allowed_repos)
    };
    query
        .fetch_all(pool)
        .await
        .map_err(|error| ApiError::Internal(format!("list agent activity: {error}")))
}

fn encode_activity_cursor(event: &ActivityEvent) -> Result<String, ApiError> {
    let bytes = serde_json::to_vec(&ActivityCursor {
        version: 1,
        created_at: event.created_at,
        id: event.id,
    })
    .map_err(|error| ApiError::Internal(format!("encode activity cursor: {error}")))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_activity_cursor(value: &str) -> Result<ActivityCursor, ApiError> {
    if value.is_empty() || value.len() > 2048 {
        return Err(ApiError::InvalidRequest("invalid activity cursor".into()));
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ApiError::InvalidRequest("invalid activity cursor".into()))?;
    let cursor: ActivityCursor = serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::InvalidRequest("invalid activity cursor".into()))?;
    if cursor.version != 1 || cursor.id <= 0 {
        return Err(ApiError::InvalidRequest("invalid activity cursor".into()));
    }
    Ok(cursor)
}

fn org_role_rank(role: &str) -> i32 {
    match role {
        "admin" => 2,
        "member" => 1,
        _ => 0,
    }
}

fn repo_perm_rank(perm: &str) -> i32 {
    match perm {
        "admin" => 3,
        "write" => 2,
        "read" => 1,
        _ => 0,
    }
}

pub async fn has_repo_permission(
    pool: &PgPool,
    repo: &RepoWithOrg,
    user_id: &str,
    min_perm: &str,
) -> Result<bool, ApiError> {
    let min = repo_perm_rank(min_perm);
    let org_role =
        sqlx::query("select role from org_memberships where org_id = $1 and user_id = $2")
            .bind(&repo.repo.org_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::Internal(format!("org role lookup: {e}")))?;
    if let Some(row) = org_role {
        let role: String = row.get("role");
        if org_role_rank(&role) >= org_role_rank("admin") {
            return Ok(true);
        }
    }
    let permission =
        sqlx::query("select permission from repo_permissions where repo_id = $1 and user_id = $2")
            .bind(&repo.repo.id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::Internal(format!("repo permission lookup: {e}")))?;
    Ok(permission.is_some_and(|row| {
        let permission: String = row.get("permission");
        repo_perm_rank(&permission) >= min
    }))
}

/// Require at least `min_role` (`admin` > `member`) in the org.
pub async fn require_org_role(
    pool: &PgPool,
    org_id: &str,
    user_id: &str,
    min_role: &str,
) -> Result<(), ApiError> {
    let min = org_role_rank(min_role);
    let row = sqlx::query("select role from org_memberships where org_id = $1 and user_id = $2")
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("org role lookup: {e}")))?;
    let Some(row) = row else {
        return Err(ApiError::Forbidden(format!("requires org {min_role} role")));
    };
    let role: String = row.get("role");
    if org_role_rank(&role) < min {
        return Err(ApiError::Forbidden(format!(
            "requires org {min_role} role (have {role})"
        )));
    }
    Ok(())
}

/// Require repo permission (`admin` > `write` > `read`). Org admins are granted.
pub async fn require_repo_permission(
    pool: &PgPool,
    repo_name: &str,
    user_id: &str,
    min_perm: &str,
) -> Result<RepoWithOrg, ApiError> {
    let clotho = get_repo_with_org(pool, repo_name)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("repo {repo_name:?} not found")))?;
    if !has_repo_permission(pool, &clotho, user_id, min_perm).await? {
        return Err(ApiError::Forbidden(format!(
            "requires repo {min_perm} permission"
        )));
    }
    Ok(clotho)
}

/// Require org admin or repo admin for destructive repo operations.
pub async fn require_repo_admin(
    pool: &PgPool,
    repo_name: &str,
    user_id: &str,
) -> Result<RepoWithOrg, ApiError> {
    let clotho = get_repo_with_org(pool, repo_name)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("repo {repo_name:?} not found")))?;

    let org_role =
        sqlx::query("select role from org_memberships where org_id = $1 and user_id = $2")
            .bind(&clotho.repo.org_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::Internal(format!("org role lookup: {e}")))?;
    if let Some(row) = org_role {
        let role: String = row.get("role");
        if org_role_rank(&role) >= org_role_rank("admin") {
            return Ok(clotho);
        }
    }

    let perm_row =
        sqlx::query("select permission from repo_permissions where repo_id = $1 and user_id = $2")
            .bind(&clotho.repo.id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::Internal(format!("repo permission lookup: {e}")))?;
    if let Some(row) = perm_row {
        let perm: String = row.get("permission");
        if repo_perm_rank(&perm) >= repo_perm_rank("admin") {
            return Ok(clotho);
        }
    }

    Err(ApiError::Forbidden(
        "requires org admin or repo admin".into(),
    ))
}

pub async fn update_repo_row(
    pool: &PgPool,
    repo_id: &str,
    req: &UpdateRepoRequest,
) -> Result<Repo, ApiError> {
    let description = req.description.as_deref();
    let visibility = req.visibility.as_deref();
    let default_branch = req.default_branch.as_deref();
    let kind = req.kind.as_deref();
    let large_file_threshold_bytes = req.large_file_threshold_bytes;
    let network_mode = req.network_mode.as_deref();
    let network_tags = req.network_tags.as_deref();

    if description.is_none()
        && visibility.is_none()
        && default_branch.is_none()
        && kind.is_none()
        && large_file_threshold_bytes.is_none()
        && network_mode.is_none()
        && network_tags.is_none()
    {
        return Err(ApiError::InvalidRequest(
            "at least one repository setting is required".into(),
        ));
    }
    if let Some(kind) = kind {
        validate_repo_kind(kind)?;
    }
    if large_file_threshold_bytes.is_some_and(|value| value < 0) {
        return Err(ApiError::InvalidRequest(
            "large_file_threshold_bytes cannot be negative".into(),
        ));
    }
    let effective_mode = network_mode.unwrap_or("public");
    if network_mode.is_some() || network_tags.is_some() {
        validate_network_policy(effective_mode, network_tags.unwrap_or(&[]))?;
    }

    if let Some(v) = visibility {
        if !matches!(v, "public" | "private" | "internal") {
            return Err(ApiError::InvalidRequest(format!(
                "visibility {v:?} must be public, private, or internal"
            )));
        }
    }

    let row = sqlx::query(
        r#"
        update repos set
            description = coalesce($2, description),
            visibility = coalesce($3, visibility),
            default_branch = coalesce($4, default_branch),
            kind = coalesce($5, kind),
            large_file_threshold_bytes = coalesce($6, large_file_threshold_bytes),
            network_mode = coalesce($7, network_mode),
            network_tags = coalesce($8, network_tags),
            updated_at = now()
        where id = $1
        returning id, org_id, name, description, visibility, kind,
                large_file_threshold_bytes, default_branch,
                network_mode, network_tags,
                forgejo_owner, forgejo_repo_id, forgejo_full_name,
                created_by, created_at, updated_at
        "#,
    )
    .bind(repo_id)
    .bind(description)
    .bind(visibility)
    .bind(default_branch)
    .bind(kind)
    .bind(large_file_threshold_bytes)
    .bind(network_mode)
    .bind(network_tags)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("update repo: {e}")))?
    .ok_or_else(|| ApiError::NotFound("repo not found".into()))?;

    Ok(Repo {
        id: row.get("id"),
        org_id: row.get("org_id"),
        name: row.get("name"),
        description: row.get("description"),
        visibility: row.get("visibility"),
        kind: row.get("kind"),
        large_file_threshold_bytes: row.get("large_file_threshold_bytes"),
        network_mode: row.get("network_mode"),
        network_tags: row.get("network_tags"),
        default_branch: row.get("default_branch"),
        forgejo_owner: row.get("forgejo_owner"),
        forgejo_repo_id: row.get("forgejo_repo_id"),
        forgejo_full_name: row.get("forgejo_full_name"),
        created_by: row.get("created_by"),
        created_at: row.get::<DateTime<Utc>, _>("created_at"),
        updated_at: row.get::<DateTime<Utc>, _>("updated_at"),
    })
}

pub async fn delete_repo_row(pool: &PgPool, repo_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from activity_events where repo_id = $1")
        .bind(repo_id)
        .execute(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("delete repo activity: {e}")))?;
    let result = sqlx::query("delete from repos where id = $1")
        .bind(repo_id)
        .execute(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("delete repo: {e}")))?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("repo not found".into()));
    }
    Ok(())
}

/// Build a Clotho-owned `RepoInfo` from the control-plane record, optionally
/// overlaying live Forgejo collaboration metadata, and always adding gateway
/// provider state and a clone URL.
pub fn build_repo_info(
    clotho: &RepoWithOrg,
    forgejo: Option<&RepoInfo>,
    public_git_url: &str,
    provider: &str,
    configured: bool,
) -> RepoInfo {
    let base = public_git_url.trim_end_matches('/');
    let git_owner = &clotho.repo.forgejo_owner;
    let name = &clotho.repo.name;
    let html_url = format!("{base}/{git_owner}/{name}");
    let clone_url = format!("{base}/{git_owner}/{name}.git");
    let mut info = forgejo.cloned().unwrap_or_default();
    info.id = clotho.repo.forgejo_repo_id.unwrap_or(info.id);
    info.clotho_id = clotho.repo.id.clone();
    info.name = name.clone();
    info.owner = clotho.org_name.clone();
    info.full_name = clotho
        .repo
        .forgejo_full_name
        .clone()
        .unwrap_or_else(|| format!("{git_owner}/{name}"));
    info.html_url = html_url;
    info.clone_url = clone_url;
    info.default_branch = clotho.repo.default_branch.clone();
    if !clotho.repo.description.is_empty() {
        info.description = clotho.repo.description.clone();
    }
    info.visibility = clotho.repo.visibility.clone();
    info.kind = clotho.repo.kind.clone();
    info.large_file_threshold_bytes = clotho.repo.large_file_threshold_bytes;
    info.network_mode = clotho.repo.network_mode.clone();
    info.network_tags = clotho.repo.network_tags.clone();
    info.updated_at = clotho.repo.updated_at.to_rfc3339();
    info.provider = provider.into();
    info.configured = configured;
    info
}

/// Return a fallback `RepoInfo` for repos that have not (yet) been adopted by
/// Forgejo or when the gateway is running without a control-plane database.
pub fn fallback_repo_info(
    name: &str,
    owner: &str,
    public_git_url: &str,
    provider: &str,
    configured: bool,
) -> RepoInfo {
    let base = public_git_url.trim_end_matches('/');
    RepoInfo {
        id: 0,
        clotho_id: String::new(),
        name: name.into(),
        full_name: format!("{owner}/{name}"),
        owner: owner.into(),
        html_url: format!("{base}/{owner}/{name}"),
        clone_url: format!("{base}/{owner}/{name}.git"),
        default_branch: "main".into(),
        description: String::new(),
        visibility: "public".into(),
        kind: "code".into(),
        large_file_threshold_bytes: 10 * 1024 * 1024,
        network_mode: "public".into(),
        network_tags: vec![],
        has_issues: true,
        has_pull_requests: true,
        open_issues_count: 0,
        open_pr_counter: 0,
        updated_at: String::new(),
        provider: provider.into(),
        configured,
    }
}

pub(crate) async fn list_users_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<UserListResponse>, ApiError> {
    let users = if let Some(pool) = &state.pool {
        list_users(pool).await?
    } else {
        vec![User {
            id: state.bootstrap.user_id.clone(),
            name: state.bootstrap.user_name.clone(),
            email: state.bootstrap.user_email.clone(),
            display_name: state.bootstrap.user_name.clone(),
            created_at: Utc::now(),
        }]
    };
    Ok(Json(UserListResponse { users }))
}

pub(crate) async fn list_orgs_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<OrgListResponse>, ApiError> {
    let orgs = if let Some(pool) = &state.pool {
        list_orgs(pool)
            .await?
            .into_iter()
            .map(OrgPublic::from)
            .collect()
    } else {
        vec![OrgPublic {
            id: state.bootstrap.org_id.clone(),
            name: state.bootstrap.org_name.clone(),
            display_name: state.bootstrap.org_display_name.clone(),
            created_by: state.bootstrap.user_id.clone(),
            created_at: Utc::now(),
        }]
    };
    Ok(Json(OrgListResponse { orgs }))
}

pub(crate) async fn create_org_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateOrgRequest>,
) -> Result<(StatusCode, Json<OrgPublic>), ApiError> {
    let auth = crate::auth::resolve_auth(&headers, &state).await?;
    let Some(pool) = &state.pool else {
        return Err(ApiError::Internal(
            "database is not configured; orgs require the control plane".into(),
        ));
    };
    let org = create_org(pool, &auth.user_id, &req, &state.bootstrap.forgejo_owner).await?;
    Ok((StatusCode::CREATED, Json(OrgPublic::from(org))))
}

pub(crate) async fn get_org_handler(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<OrgDetailResponse>, ApiError> {
    let Some(pool) = &state.pool else {
        if org == state.bootstrap.org_name || org == state.bootstrap.org_id {
            return Ok(Json(OrgDetailResponse {
                org: OrgPublic {
                    id: state.bootstrap.org_id.clone(),
                    name: state.bootstrap.org_name.clone(),
                    display_name: state.bootstrap.org_display_name.clone(),
                    created_by: state.bootstrap.user_id.clone(),
                    created_at: Utc::now(),
                },
                members: vec![OrgMembership {
                    org_id: state.bootstrap.org_id.clone(),
                    user_id: state.bootstrap.user_id.clone(),
                    role: "admin".into(),
                    user_name: state.bootstrap.user_name.clone(),
                    user_display_name: state.bootstrap.user_name.clone(),
                }],
            }));
        }
        return Err(ApiError::NotFound(format!("org {org:?} not found")));
    };
    match get_org_with_members(pool, &org).await? {
        Some(OrgWithMembers { org, members }) => Ok(Json(OrgDetailResponse {
            org: OrgPublic::from(org),
            members,
        })),
        None => Err(ApiError::NotFound(format!("org {org:?} not found"))),
    }
}

pub(crate) async fn list_org_repos_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(org): Path<String>,
    Query(query): Query<crate::repos::RepoListQuery>,
) -> Result<Json<crate::repos::RepoListResponse>, ApiError> {
    let principal = crate::auth::resolve_optional_human_auth(&headers, &state).await?;
    let provider = state.actions.default_provider();
    let configured = state.actions.provider_configured(&provider);
    let base_url = state.public_git_url.clone();

    let repos = if let Some(pool) = &state.pool {
        let clotho = list_accessible_repos_for_org(
            pool,
            &org,
            principal.as_ref().map(|auth| auth.user_id.as_str()),
        )
        .await?;
        // Authorization/filtering is complete before the provider overlay.
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
            .map(|r| {
                build_repo_info(
                    &r,
                    forgejo_by_name.get(&r.repo.name),
                    &base_url,
                    &provider,
                    configured,
                )
            })
            .collect()
    } else {
        return Err(ApiError::Internal(
            "repository lists require the control plane".into(),
        ));
    };

    Ok(Json(crate::repos::paginate_repos(repos, query)?))
}

pub(crate) async fn list_activity_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(query): Query<ActivityQuery>,
) -> Result<Json<ActivityListResponse>, ApiError> {
    let agent = crate::agent_rest::authorize_if_agent(&headers, &state, "", "get_activity").await?;
    let principal = if agent.is_none() {
        crate::auth::resolve_optional_human_auth(&headers, &state).await?
    } else {
        None
    };
    let limit = query.limit.unwrap_or(DEFAULT_ACTIVITY_PAGE_SIZE);
    if !(1..=MAX_ACTIVITY_PAGE_SIZE).contains(&limit) {
        return Err(ApiError::InvalidRequest(format!(
            "limit must be between 1 and {MAX_ACTIVITY_PAGE_SIZE}"
        )));
    }
    let cursor = query
        .cursor
        .as_deref()
        .map(decode_activity_cursor)
        .transpose()?;
    let mut events = if let Some(pool) = &state.pool {
        if let Some(agent) = &agent {
            list_activity_for_agent(
                pool,
                (limit + 1) as i64,
                cursor.as_ref(),
                &agent.allowed_repos,
            )
            .await?
        } else {
            list_activity(
                pool,
                (limit + 1) as i64,
                cursor.as_ref(),
                principal.as_ref().map(|auth| auth.user_id.as_str()),
            )
            .await?
        }
    } else {
        vec![]
    };
    let has_more = events.len() > limit;
    if has_more {
        events.pop();
    }
    let next_cursor = if has_more {
        events.last().map(encode_activity_cursor).transpose()?
    } else {
        None
    };
    Ok(Json(ActivityListResponse {
        events,
        next_cursor,
    }))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::*;

    fn test_database_url() -> Option<String> {
        let url = std::env::var("CLOTHO_CONTROL_PLANE_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://clotho:clotho-dev@localhost:5432/clotho".into());
        if url.trim().is_empty() {
            None
        } else {
            Some(url)
        }
    }

    async fn pool() -> Option<PgPool> {
        let url = test_database_url()?;
        crate::init_db(&url).await.ok()
    }

    fn test_bootstrap() -> Bootstrap {
        let suffix = Uuid::new_v4().to_string().replace('-', "");
        Bootstrap {
            user_id: format!("testuser-{suffix}"),
            user_name: format!("testuser-{suffix}"),
            user_email: format!("test-{suffix}@clotho.internal"),
            org_id: format!("testorg-{suffix}"),
            org_name: format!("testorg-{suffix}"),
            org_display_name: "Test Org".into(),
            forgejo_owner: "clotho".into(),
        }
    }

    async fn cleanup(pool: &PgPool, org: &str, repo: &str, user: &str) {
        let _ = sqlx::query("delete from activity_events where org_id = $1 or repo_id in (select id from repos where name = $2)")
            .bind(org)
            .bind(repo)
            .execute(pool)
            .await;
        let _ = sqlx::query(
            "delete from repo_permissions where repo_id in (select id from repos where name = $1)",
        )
        .bind(repo)
        .execute(pool)
        .await;
        let _ = sqlx::query("delete from repos where name = $1")
            .bind(repo)
            .execute(pool)
            .await;
        let _ = sqlx::query("delete from org_memberships where org_id = $1")
            .bind(org)
            .execute(pool)
            .await;
        let _ = sqlx::query("delete from orgs where id = $1")
            .bind(org)
            .execute(pool)
            .await;
        let _ = sqlx::query("delete from users where id = $1")
            .bind(user)
            .execute(pool)
            .await;
    }

    #[tokio::test]
    async fn ensure_bootstrap_creates_user_and_org() {
        let Some(pool) = pool().await else { return };
        let b = test_bootstrap();
        ensure_bootstrap(&pool, &b).await.unwrap();

        let users = list_users(&pool).await.unwrap();
        assert!(users.iter().any(|u| u.name == b.user_name));

        let orgs = list_orgs(&pool).await.unwrap();
        assert!(orgs.iter().any(|o| o.name == b.org_name));

        cleanup(&pool, &b.org_id, "no-such-repo", &b.user_id).await;
    }

    #[tokio::test]
    async fn create_org_and_get_with_members() {
        let Some(pool) = pool().await else { return };
        let b = test_bootstrap();
        ensure_bootstrap(&pool, &b).await.unwrap();

        let suffix = Uuid::new_v4().to_string().replace('-', "");
        let req = CreateOrgRequest {
            name: format!("testorg-{suffix}"),
            display_name: format!("Test Org {suffix}"),
            git_owner: "clotho".into(),
        };
        let created = create_org(&pool, &b.user_id, &req, &b.forgejo_owner)
            .await
            .unwrap();
        assert_eq!(created.display_name, req.display_name);

        let detail = get_org_with_members(&pool, &created.name)
            .await
            .unwrap()
            .expect("org should exist");
        assert_eq!(detail.org.name, created.name);
        assert!(detail.members.iter().any(|m| m.role == "admin"));

        cleanup(&pool, &created.id, "no-such-repo", &b.user_id).await;
        cleanup(&pool, &b.org_id, "no-such-repo", &b.user_id).await;
    }

    #[tokio::test]
    async fn insert_and_retrieve_repo() {
        let Some(pool) = pool().await else { return };
        let b = test_bootstrap();
        ensure_bootstrap(&pool, &b).await.unwrap();

        let suffix = Uuid::new_v4().to_string().replace('-', "");
        let req = CreateRepoRequest {
            name: format!("testrepo-{suffix}"),
            description: "a test repo".into(),
            visibility: "public".into(),
            kind: "model".into(),
            large_file_threshold_bytes: None,
            network_mode: "public".into(),
            network_tags: vec![],
            default_branch: "main".into(),
            owner_org: String::new(),
        };
        let forgejo = RepoInfo {
            id: 123,
            name: req.name.clone(),
            full_name: format!("clotho/{}", req.name),
            ..Default::default()
        };
        let resolved = resolve_org(&pool, &b, &req.owner_org).await.unwrap();
        let clotho = insert_repo(&pool, &b.user_id, &req, &resolved, &forgejo)
            .await
            .unwrap();

        assert_eq!(clotho.repo.name, req.name);

        let all = list_repos_with_orgs(&pool).await.unwrap();
        assert!(all.iter().any(|r| r.repo.name == req.name));

        let one = get_repo_with_org(&pool, &req.name)
            .await
            .unwrap()
            .expect("repo should exist");
        assert_eq!(one.repo.visibility, "public");
        assert_eq!(one.repo.kind, "model");
        assert_eq!(one.repo.large_file_threshold_bytes, 1024 * 1024);

        let events = list_activity(&pool, 10, None, Some(&b.user_id))
            .await
            .unwrap();
        assert!(events.iter().any(|e| e.event_type == "repo.created"));

        cleanup(&pool, &resolved.0, &req.name, &b.user_id).await;
    }

    #[test]
    fn activity_cursor_round_trips_and_rejects_invalid_values() {
        let event = ActivityEvent {
            id: 42,
            actor_id: "user".into(),
            org_id: None,
            repo_id: None,
            event_type: "repo.created".into(),
            payload: serde_json::json!({}),
            created_at: Utc::now(),
        };
        let encoded = encode_activity_cursor(&event).unwrap();
        let decoded = decode_activity_cursor(&encoded).unwrap();
        assert_eq!(decoded.version, 1);
        assert_eq!(decoded.created_at, event.created_at);
        assert_eq!(decoded.id, event.id);

        assert!(decode_activity_cursor("").is_err());
        assert!(decode_activity_cursor("not-a-cursor").is_err());
        assert!(decode_activity_cursor(&"a".repeat(2049)).is_err());
    }

    #[test]
    fn build_repo_info_computes_urls() {
        let clotho = RepoWithOrg {
            repo: Repo {
                id: "r1".into(),
                org_id: "o1".into(),
                name: "weave".into(),
                description: "woven".into(),
                visibility: "public".into(),
                kind: "code".into(),
                large_file_threshold_bytes: 10 * 1024 * 1024,
                network_mode: "public".into(),
                network_tags: vec![],
                default_branch: "main".into(),
                forgejo_owner: "clotho".into(),
                forgejo_repo_id: Some(42),
                forgejo_full_name: Some("clotho/weave".into()),
                created_by: "u1".into(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            org_name: "weavers".into(),
            org_display_name: "Weavers".into(),
        };
        let info = build_repo_info(&clotho, None, "http://localhost:13000", "daytona", true);
        assert_eq!(info.name, "weave");
        assert_eq!(info.owner, "weavers");
        assert_eq!(info.clone_url, "http://localhost:13000/clotho/weave.git");
        assert!(info.configured);
    }
}
