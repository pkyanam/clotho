//! Repository milestone endpoints — proxied from the internal collaboration provider.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth;
use crate::control;
use crate::error::ApiError;
use crate::forgejo::MilestoneInfo;
use crate::AppState;

#[derive(Serialize)]
pub struct MilestoneListResponse {
    pub milestones: Vec<MilestoneInfo>,
}

#[derive(Deserialize)]
pub struct CreateMilestoneRequest {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub due_on: Option<String>,
}

pub async fn list_milestones(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<MilestoneListResponse>, ApiError> {
    auth::require_repo_read(&headers, &state, &name).await?;
    let milestones = state.forgejo.list_milestones(&name).await?;
    Ok(Json(MilestoneListResponse { milestones }))
}

pub async fn create_milestone(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(req): Json<CreateMilestoneRequest>,
) -> Result<(StatusCode, Json<MilestoneInfo>), ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    if let Some(pool) = &state.pool {
        control::require_repo_permission(pool, &name, &auth.user_id, "write").await?;
    }
    if req.title.trim().is_empty() {
        return Err(ApiError::InvalidRequest("title is required".into()));
    }
    let milestone = state
        .forgejo
        .create_milestone(
            &name,
            req.title.trim(),
            Some(req.description.trim()),
            req.due_on.as_deref(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(milestone)))
}
