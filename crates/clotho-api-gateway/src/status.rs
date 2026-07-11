//! Repository collaboration status facade: branches and commit statuses.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;

use crate::auth;
use crate::error::ApiError;
use crate::forgejo::{BranchInfo, CommitStatusInfo};
use crate::AppState;

#[derive(Serialize)]
pub struct BranchListResponse {
    pub branches: Vec<BranchInfo>,
}

#[derive(Serialize)]
pub struct CommitStatusListResponse {
    pub statuses: Vec<CommitStatusInfo>,
}

pub async fn branches(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<BranchListResponse>, ApiError> {
    auth::require_repo_read(&headers, &state, &name).await?;
    let branches = state.forgejo.list_branches(&name).await?;
    Ok(Json(BranchListResponse { branches }))
}

pub async fn commit_statuses(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, sha)): Path<(String, String)>,
) -> Result<Json<CommitStatusListResponse>, ApiError> {
    auth::require_repo_read(&headers, &state, &name).await?;
    let statuses = state.forgejo.commit_statuses(&name, &sha).await?;
    Ok(Json(CommitStatusListResponse { statuses }))
}
