//! Narrow Stage 22 authorization bridge for REST calls carrying a Clotho
//! agent bearer.
//!
//! The agent gateway owns token storage and scope semantics (ADR-0016). The
//! REST edge therefore asks that service to revalidate the original bearer
//! against a handler-selected repository and MCP tool. External headers never
//! choose the expected tool and no privileged human/service credential is used
//! as the agent's authority.

use axum::http::HeaderMap;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::AppState;

const AGENT_TOKEN_PREFIX: &str = "clotho_agt_";

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AuthorizedAgent {
    pub agent_id: String,
    pub token_id: String,
    pub agent: String,
    #[serde(default)]
    pub allowed_repos: Vec<String>,
}

impl AuthorizedAgent {
    pub fn principal_id(&self) -> String {
        format!("agent:{}:token:{}", self.agent_id, self.token_id)
    }

    pub fn may_touch_repo(&self, repo: &str) -> bool {
        self.allowed_repos
            .iter()
            .any(|allowed| allowed == "*" || allowed == repo)
    }
}

#[derive(Serialize)]
struct AuthorizeAgentRequest<'a> {
    token: &'a str,
    repo: &'a str,
    tool: &'a str,
}

/// Return `Some` only when the request carries an agent bearer. Human or
/// absent credentials return `None` and continue through the human/anonymous
/// path. Agent credentials always succeed or fail here; they never fall back
/// to a human bootstrap identity.
pub(crate) async fn authorize_if_agent(
    headers: &HeaderMap,
    state: &AppState,
    repo: &str,
    tool: &'static str,
) -> Result<Option<AuthorizedAgent>, ApiError> {
    let Some(token) = crate::auth::extract_bearer(headers) else {
        return Ok(None);
    };
    if !token.starts_with(AGENT_TOKEN_PREFIX) {
        return Ok(None);
    }
    if tool.is_empty() {
        return Err(ApiError::Internal(
            "agent authorization requires a handler-owned tool".into(),
        ));
    }
    if state.agent_admin_token.trim().is_empty() || state.agent_gateway_url.trim().is_empty() {
        return Err(ApiError::Internal(
            "agent authorization service is not configured".into(),
        ));
    }

    let response = state
        .http
        .post(format!("{}/admin/v1/authorize", state.agent_gateway_url))
        .bearer_auth(&state.agent_admin_token)
        .json(&AuthorizeAgentRequest {
            token: &token,
            repo,
            tool,
        })
        .send()
        .await
        .map_err(|error| ApiError::Upstream(format!("agent authorization request: {error}")))?;
    let status = response.status();
    match status {
        StatusCode::OK => response
            .json::<AuthorizedAgent>()
            .await
            .map(Some)
            .map_err(|error| {
                ApiError::Upstream(format!("decode agent authorization response: {error}"))
            }),
        StatusCode::UNAUTHORIZED => Err(ApiError::Unauthorized(
            "agent credential is invalid or inactive".into(),
        )),
        StatusCode::FORBIDDEN => Err(ApiError::Forbidden(
            "agent credential does not authorize this operation".into(),
        )),
        _ => Err(ApiError::Upstream(format!(
            "agent authorization returned HTTP {}",
            status.as_u16()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_principal_and_repo_scope_are_stable() {
        let exact = AuthorizedAgent {
            agent_id: "agent-id".into(),
            token_id: "token-id".into(),
            agent: "weaver".into(),
            allowed_repos: vec!["repo-a".into()],
        };
        assert_eq!(exact.principal_id(), "agent:agent-id:token:token-id");
        assert!(exact.may_touch_repo("repo-a"));
        assert!(!exact.may_touch_repo("repo-b"));

        let wildcard = AuthorizedAgent {
            allowed_repos: vec!["*".into()],
            ..exact
        };
        assert!(wildcard.may_touch_repo("repo-b"));
    }
}
