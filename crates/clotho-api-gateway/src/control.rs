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
    pub orgs: Vec<Org>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OrgDetailResponse {
    pub org: Org,
    pub members: Vec<OrgMembership>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActivityListResponse {
    pub events: Vec<ActivityEvent>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateOrgRequest {
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub forgejo_owner: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateRepoRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_visibility")]
    pub visibility: String,
    #[serde(default = "default_default_branch")]
    pub default_branch: String,
    #[serde(default)]
    pub owner_org: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ActivityQuery {
    #[serde(default = "default_activity_limit")]
    pub limit: u32,
}

fn default_visibility() -> String {
    "public".into()
}

fn default_default_branch() -> String {
    "main".into()
}

fn default_activity_limit() -> u32 {
    50
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
    b: &Bootstrap,
    req: &CreateOrgRequest,
) -> Result<Org, ApiError> {
    valid_name(&req.name)?;
    let name = req.name.trim().to_lowercase();
    let display_name = if req.display_name.is_empty() {
        name.clone()
    } else {
        req.display_name.trim().into()
    };
    let forgejo_owner = if req.forgejo_owner.is_empty() {
        b.forgejo_owner.clone()
    } else {
        req.forgejo_owner.trim().into()
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
    .bind(&b.user_id)
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
        .bind(&b.user_id)
        .execute(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("create org membership: {e}")))?;

    log_activity(
        pool,
        ActivityEventInput {
            actor_id: b.user_id.clone(),
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
    sqlx::query_as::<_, RepoWithOrg>(
        r#"
        select
            r.id, r.org_id, r.name, r.description, r.visibility,
            r.default_branch, r.forgejo_owner, r.forgejo_repo_id,
            r.forgejo_full_name, r.created_by, r.created_at, r.updated_at,
            o.name as org_name, o.display_name as org_display_name
        from repos r
        join orgs o on o.id = r.org_id
        where r.name = $1
        order by r.updated_at desc
        limit 1
        "#,
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("get repo: {e}")))
}

pub async fn list_repos_with_orgs(pool: &PgPool) -> Result<Vec<RepoWithOrg>, ApiError> {
    sqlx::query_as::<_, RepoWithOrg>(
        r#"
        select
            r.id, r.org_id, r.name, r.description, r.visibility,
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
            r.id, r.org_id, r.name, r.description, r.visibility,
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
    b: &Bootstrap,
    req: &CreateRepoRequest,
    (org_id, org_name, forgejo_owner): &(String, String, String),
    forgejo_repo: &RepoInfo,
) -> Result<RepoWithOrg, ApiError> {
    valid_name(&req.name)?;
    let id = Uuid::new_v4().to_string();
    let full_name = format!("{forgejo_owner}/{}", req.name);

    let row = sqlx::query(
        r#"
        insert into repos (
            id, org_id, name, description, visibility, default_branch,
            forgejo_owner, forgejo_repo_id, forgejo_full_name, created_by
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        returning id, org_id, name, description, visibility, default_branch,
                forgejo_owner, forgejo_repo_id, forgejo_full_name,
                created_by, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(org_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.visibility)
    .bind(&req.default_branch)
    .bind(forgejo_owner)
    .bind(forgejo_repo.id)
    .bind(&full_name)
    .bind(&b.user_id)
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
    .bind(&b.user_id)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("insert repo permissions: {e}")))?;

    log_activity(
        pool,
        ActivityEventInput {
            actor_id: b.user_id.clone(),
            org_id: Some(org_id.clone()),
            repo_id: Some(repo.id.clone()),
            event_type: "repo.created".into(),
            payload: serde_json::json!({
                "repo_name": req.name,
                "org_name": org_name,
                "visibility": req.visibility,
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

pub async fn list_activity(pool: &PgPool, limit: i64) -> Result<Vec<ActivityEvent>, ApiError> {
    sqlx::query_as::<_, ActivityEvent>(
        r#"
        select * from activity_events
        order by created_at desc
        limit $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("list activity: {e}")))
}

/// Build a Clotho-owned `RepoInfo` from the control-plane record, optionally
/// overlaying live Forgejo collaboration metadata, and always adding gateway
/// provider state and a clone URL.
pub fn build_repo_info(
    clotho: &RepoWithOrg,
    forgejo: Option<&RepoInfo>,
    base_url: &str,
    provider: &str,
    configured: bool,
) -> RepoInfo {
    let base = base_url.trim_end_matches('/');
    let owner = &clotho.repo.forgejo_owner;
    let name = &clotho.repo.name;
    let html_url = format!("{base}/{owner}/{name}");
    let clone_url = format!("{base}/{owner}/{name}.git");
    let mut info = forgejo.cloned().unwrap_or_default();
    info.id = clotho.repo.forgejo_repo_id.unwrap_or(info.id);
    info.clotho_id = clotho.repo.id.clone();
    info.name = name.clone();
    info.owner = clotho.org_name.clone();
    info.full_name = clotho
        .repo
        .forgejo_full_name
        .clone()
        .unwrap_or_else(|| format!("{owner}/{name}"));
    info.html_url = html_url;
    info.clone_url = clone_url;
    info.default_branch = clotho.repo.default_branch.clone();
    if !clotho.repo.description.is_empty() {
        info.description = clotho.repo.description.clone();
    }
    info.visibility = clotho.repo.visibility.clone();
    info.provider = provider.into();
    info.configured = configured;
    info
}

/// Return a fallback `RepoInfo` for repos that have not (yet) been adopted by
/// Forgejo or when the gateway is running without a control-plane database.
pub fn fallback_repo_info(
    name: &str,
    owner: &str,
    base_url: &str,
    provider: &str,
    configured: bool,
) -> RepoInfo {
    let base = base_url.trim_end_matches('/');
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
        list_orgs(pool).await?
    } else {
        vec![Org {
            id: state.bootstrap.org_id.clone(),
            name: state.bootstrap.org_name.clone(),
            display_name: state.bootstrap.org_display_name.clone(),
            forgejo_owner: state.bootstrap.forgejo_owner.clone(),
            created_by: state.bootstrap.user_id.clone(),
            created_at: Utc::now(),
        }]
    };
    Ok(Json(OrgListResponse { orgs }))
}

pub(crate) async fn create_org_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateOrgRequest>,
) -> Result<(StatusCode, Json<Org>), ApiError> {
    let Some(pool) = &state.pool else {
        return Err(ApiError::Internal(
            "database is not configured; orgs require the control plane".into(),
        ));
    };
    let org = create_org(pool, &state.bootstrap, &req).await?;
    Ok((StatusCode::CREATED, Json(org)))
}

pub(crate) async fn get_org_handler(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<OrgDetailResponse>, ApiError> {
    let Some(pool) = &state.pool else {
        if org == state.bootstrap.org_name || org == state.bootstrap.org_id {
            return Ok(Json(OrgDetailResponse {
                org: Org {
                    id: state.bootstrap.org_id.clone(),
                    name: state.bootstrap.org_name.clone(),
                    display_name: state.bootstrap.org_display_name.clone(),
                    forgejo_owner: state.bootstrap.forgejo_owner.clone(),
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
        Some(OrgWithMembers { org, members }) => Ok(Json(OrgDetailResponse { org, members })),
        None => Err(ApiError::NotFound(format!("org {org:?} not found"))),
    }
}

pub(crate) async fn list_org_repos_handler(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<crate::repos::RepoListResponse>, ApiError> {
    let provider = state.actions.default_provider();
    let configured = state.actions.provider_configured(&provider);
    let base_url = state.forgejo.config().base_url.clone();

    let mut repos = if let Some(pool) = &state.pool {
        let clotho = list_repos_for_org(pool, &org).await?;
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
        vec![]
    };

    if repos.is_empty() {
        // Fallback to Forgejo if the control-plane table is empty or the DB is
        // not configured, so existing stack behavior still works.
        repos = state
            .forgejo
            .list_repos()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|mut r| {
                r.provider = provider.clone();
                r.configured = configured;
                r
            })
            .collect();
    }

    Ok(Json(crate::repos::RepoListResponse { repos }))
}

pub(crate) async fn list_activity_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ActivityQuery>,
) -> Result<Json<ActivityListResponse>, ApiError> {
    let limit = query.limit.clamp(1, 500) as i64;
    let events = if let Some(pool) = &state.pool {
        list_activity(pool, limit).await?
    } else {
        vec![]
    };
    Ok(Json(ActivityListResponse { events }))
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
        Bootstrap {
            user_id: "testuser".into(),
            user_name: "testuser".into(),
            user_email: "test@clotho.internal".into(),
            org_id: "testorg".into(),
            org_name: "testorg".into(),
            org_display_name: "Test Org".into(),
            forgejo_owner: "clotho".into(),
        }
    }

    async fn cleanup(pool: &PgPool, org: &str, repo: &str) {
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
            .bind(test_bootstrap().user_id)
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

        cleanup(&pool, &b.org_id, "no-such-repo").await;
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
            forgejo_owner: "clotho".into(),
        };
        let created = create_org(&pool, &b, &req).await.unwrap();
        assert_eq!(created.display_name, req.display_name);

        let detail = get_org_with_members(&pool, &created.name)
            .await
            .unwrap()
            .expect("org should exist");
        assert_eq!(detail.org.name, created.name);
        assert!(detail.members.iter().any(|m| m.role == "admin"));

        cleanup(&pool, &created.id, "no-such-repo").await;
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
        let clotho = insert_repo(&pool, &b, &req, &resolved, &forgejo)
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

        let events = list_activity(&pool, 10).await.unwrap();
        assert!(events.iter().any(|e| e.event_type == "repo.created"));

        cleanup(&pool, &resolved.0, &req.name).await;
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
