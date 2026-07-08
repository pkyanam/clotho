//! Clotho collaboration facade for issues. Forgejo backs the data in Stage 9,
//! but callers receive Clotho-owned JSON shapes rather than reaching around
//! the gateway.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::forgejo::{CommentInfo, IssueInfo};
use crate::AppState;

#[derive(Deserialize)]
pub struct IssuesQuery {
    #[serde(default = "default_issue_state")]
    pub state: String,
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
}

#[derive(Deserialize)]
pub struct CreateCommentRequest {
    pub body: String,
}

pub async fn list_issues(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<IssuesQuery>,
) -> Result<Json<IssueListResponse>, ApiError> {
    validate_state(&query.state)?;
    let issues = state.forgejo.list_issues(&name, &query.state).await?;
    Ok(Json(IssueListResponse { issues }))
}

pub async fn create_issue(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<CreateIssueRequest>,
) -> Result<(StatusCode, Json<IssueInfo>), ApiError> {
    if req.title.trim().is_empty() {
        return Err(ApiError::InvalidRequest("title is required".into()));
    }
    let issue = state
        .forgejo
        .create_issue(&name, req.title.trim(), req.body.trim())
        .await?;
    Ok((StatusCode::CREATED, Json(issue)))
}

pub async fn get_issue(
    State(state): State<Arc<AppState>>,
    Path((name, number)): Path<(String, i64)>,
) -> Result<Json<IssueDetailResponse>, ApiError> {
    let (issue, comments) = tokio::try_join!(
        state.forgejo.get_issue(&name, number),
        state.forgejo.list_issue_comments(&name, number)
    )?;
    Ok(Json(IssueDetailResponse { issue, comments }))
}

pub async fn create_comment(
    State(state): State<Arc<AppState>>,
    Path((name, number)): Path<(String, i64)>,
    Json(req): Json<CreateCommentRequest>,
) -> Result<(StatusCode, Json<CommentInfo>), ApiError> {
    if req.body.trim().is_empty() {
        return Err(ApiError::InvalidRequest("body is required".into()));
    }
    let comment = state
        .forgejo
        .comment_on_issue(&name, number, req.body.trim())
        .await?;
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
