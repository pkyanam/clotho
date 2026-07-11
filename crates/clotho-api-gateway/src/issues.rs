//! Clotho collaboration facade for issues. Forgejo backs the data in Stage 9,
//! but callers receive Clotho-owned JSON shapes rather than reaching around
//! the gateway.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth;
use crate::control;
use crate::error::ApiError;
use crate::forgejo::{CommentInfo, IssueInfo, IssueUpdate};
use crate::notifications;
use crate::AppState;

#[derive(Deserialize)]
pub struct IssuesQuery {
    #[serde(default = "default_issue_state")]
    pub state: String,
    #[serde(default)]
    pub labels: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub milestone: Option<i64>,
}

fn default_issue_state() -> String {
    "open".into()
}

#[derive(Serialize)]
pub struct IssueListResponse {
    pub issues: Vec<IssueInfo>,
}

#[derive(Serialize)]
pub struct IssueDetailResponse {
    pub issue: IssueInfo,
    pub comments: Vec<CommentInfo>,
}

#[derive(Deserialize)]
pub struct CreateIssueRequest {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignees: Vec<String>,
    #[serde(default)]
    pub milestone: Option<i64>,
}

#[derive(Deserialize)]
pub struct UpdateIssueRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    #[serde(default)]
    pub assignees: Option<Vec<String>>,
    /// Set to a milestone id, or JSON `null` to clear.
    #[serde(default)]
    pub milestone: Option<Option<i64>>,
}

#[derive(Deserialize)]
pub struct CreateCommentRequest {
    pub body: String,
}

pub async fn list_issues(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(query): Query<IssuesQuery>,
) -> Result<Json<IssueListResponse>, ApiError> {
    auth::require_repo_read_for_tool(&headers, &state, &name, "list_issues").await?;
    validate_state(&query.state)?;
    let issues = state
        .forgejo
        .list_issues(
            &name,
            &query.state,
            query.labels.as_deref(),
            query.assignee.as_deref(),
            query.milestone,
        )
        .await?;
    Ok(Json(IssueListResponse { issues }))
}

pub async fn create_issue(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(req): Json<CreateIssueRequest>,
) -> Result<(StatusCode, Json<IssueInfo>), ApiError> {
    let (_repo, actor) =
        auth::require_repo_write_for_tool(&headers, &state, &name, "create_issue").await?;
    if req.title.trim().is_empty() {
        return Err(ApiError::InvalidRequest("title is required".into()));
    }
    let issue = state
        .forgejo
        .create_issue(
            &name,
            req.title.trim(),
            req.body.trim(),
            &req.labels,
            &req.assignees,
            req.milestone,
        )
        .await?;

    if let Some(pool) = &state.pool {
        if !req.assignees.is_empty() {
            notifications::notify_issue_assigned(
                pool,
                &name,
                issue.number,
                &issue.title,
                &req.assignees,
                Some(actor.actor_name()),
            )
            .await;
        }
    }

    Ok((StatusCode::CREATED, Json(issue)))
}

pub async fn update_issue(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, number)): Path<(String, i64)>,
    Json(req): Json<UpdateIssueRequest>,
) -> Result<Json<IssueInfo>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    if let Some(pool) = &state.pool {
        control::require_repo_permission(pool, &name, &auth.user_id, "write").await?;
    }
    if let Some(ref state_val) = req.state {
        validate_issue_state(state_val)?;
    }
    let issue = state
        .forgejo
        .update_issue(
            &name,
            number,
            IssueUpdate {
                title: req.title.as_deref().map(str::trim),
                body: req.body.as_deref().map(str::trim),
                state: req.state.as_deref(),
                assignees: req.assignees.as_deref(),
                labels: req.labels.as_deref(),
                milestone: req.milestone,
            },
        )
        .await?;

    if let Some(pool) = &state.pool {
        if let Some(assignees) = &req.assignees {
            notifications::notify_issue_assigned(
                pool,
                &name,
                number,
                &issue.title,
                assignees,
                Some(&auth.user_name),
            )
            .await;
        }
    }

    Ok(Json(issue))
}

pub async fn get_issue(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, number)): Path<(String, i64)>,
) -> Result<Json<IssueDetailResponse>, ApiError> {
    auth::require_repo_read(&headers, &state, &name).await?;
    let (issue, comments) = tokio::try_join!(
        state.forgejo.get_issue(&name, number),
        state.forgejo.list_issue_comments(&name, number)
    )?;
    Ok(Json(IssueDetailResponse { issue, comments }))
}

pub async fn create_comment(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, number)): Path<(String, i64)>,
    Json(req): Json<CreateCommentRequest>,
) -> Result<(StatusCode, Json<CommentInfo>), ApiError> {
    auth::require_repo_write_for_tool(&headers, &state, &name, "comment_issue").await?;
    if req.body.trim().is_empty() {
        return Err(ApiError::InvalidRequest("body is required".into()));
    }

    let issue = state.forgejo.get_issue(&name, number).await?;
    let comment = state
        .forgejo
        .comment_on_issue(&name, number, req.body.trim())
        .await?;

    if let Some(pool) = &state.pool {
        notifications::notify_issue_comment(
            pool,
            &name,
            number,
            &issue.title,
            &issue.user.login,
            &comment.user.login,
            req.body.trim(),
        )
        .await;
    }

    Ok((StatusCode::CREATED, Json(comment)))
}

fn validate_state(state: &str) -> Result<(), ApiError> {
    if matches!(state, "open" | "closed" | "all") {
        Ok(())
    } else {
        Err(ApiError::InvalidRequest(format!(
            "state {state:?} must be open, closed, or all"
        )))
    }
}

fn validate_issue_state(state: &str) -> Result<(), ApiError> {
    if matches!(state, "open" | "closed") {
        Ok(())
    } else {
        Err(ApiError::InvalidRequest(format!(
            "state {state:?} must be open or closed"
        )))
    }
}
