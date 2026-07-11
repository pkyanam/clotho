//! Repository label endpoints — proxied from the internal collaboration provider.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth;
use crate::control;
use crate::error::ApiError;
use crate::forgejo::LabelInfo;
use crate::AppState;

#[derive(Serialize)]
pub struct LabelListResponse {
    pub labels: Vec<LabelInfo>,
}

#[derive(Deserialize)]
pub struct CreateLabelRequest {
    pub name: String,
    pub color: String,
    #[serde(default)]
    pub description: String,
}

pub async fn list_labels(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<LabelListResponse>, ApiError> {
    auth::require_repo_read(&headers, &state, &name).await?;
    let labels = state.forgejo.list_labels(&name).await?;
    Ok(Json(LabelListResponse { labels }))
}

pub async fn create_label(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(req): Json<CreateLabelRequest>,
) -> Result<(StatusCode, Json<LabelInfo>), ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    if let Some(pool) = &state.pool {
        control::require_repo_permission(pool, &name, &auth.user_id, "write").await?;
    }
    if req.name.trim().is_empty() {
        return Err(ApiError::InvalidRequest("name is required".into()));
    }
    if req.color.trim().is_empty() {
        return Err(ApiError::InvalidRequest("color is required".into()));
    }
    let label = state
        .forgejo
        .create_label(
            &name,
            req.name.trim(),
            req.color.trim(),
            Some(req.description.trim()),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(label)))
}
