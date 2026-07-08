//! Repository collaboration status facade: branches and commit statuses.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

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
    Path(name): Path<String>,
) -> Result<Json<BranchListResponse>, ApiError> {
    let branches = state.forgejo.list_branches(&name).await?;
    Ok(Json(BranchListResponse { branches }))
}

pub async fn commit_statuses(
    State(state): State<Arc<AppState>>,
    Path((name, sha)): Path<(String, String)>,
) -> Result<Json<CommitStatusListResponse>, ApiError> {
    let statuses = state.forgejo.commit_statuses(&name, &sha).await?;
    Ok(Json(CommitStatusListResponse { statuses }))
}
