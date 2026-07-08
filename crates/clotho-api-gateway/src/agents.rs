//! Agent-presence endpoint: which agent sessions touched a repo recently.
//!
//! The agent gateway owns agent identity and the audit log (ADR-0005); this
//! gateway does not reach into that Postgres schema. Instead it proxies the
//! agent gateway's admin sessions endpoint with a service-to-service admin
//! token, keeping one owner per data set (ADR-0007). The web app polls this.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::error::ApiError;
use crate::AppState;

#[derive(Deserialize)]
pub struct SessionsQuery {
    #[serde(default = "default_sessions_limit")]
    pub limit: i64,
    /// Look-back window in seconds (default: 7 days).
    #[serde(default = "default_sessions_within_secs")]
    pub within_secs: i64,
}

fn default_sessions_limit() -> i64 {
    20
}

fn default_sessions_within_secs() -> i64 {
    7 * 24 * 3600
}

pub async fn repo_sessions(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<SessionsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let url = format!(
        "{}/admin/v1/repos/{name}/sessions?limit={}&within_secs={}",
        state.agent_gateway_url, query.limit, query.within_secs
    );
    let response = state
        .http
        .get(&url)
        .bearer_auth(&state.agent_admin_token)
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("agent-gateway: {e}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError::Upstream(format!(
            "agent-gateway: sessions returned {status}: {body}"
        )));
    }
    let sessions: serde_json::Value = response
        .json()
        .await
        .map_err(|e| ApiError::Upstream(format!("agent-gateway: invalid sessions body: {e}")))?;
    Ok(Json(serde_json::json!({ "sessions": sessions })))
}
