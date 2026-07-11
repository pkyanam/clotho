//! Bootstrap AuthProvider — ADR-0015 human tokens + open local/dev fallback.

use async_trait::async_trait;
use axum::http::HeaderMap;

use crate::auth::{extract_bearer, validate_clotho_token, AuthContext};
use crate::error::ApiError;
use crate::AppState;

use super::{AuthProvider, AuthProviderId};

pub struct BootstrapAuthProvider;

#[async_trait]
impl AuthProvider for BootstrapAuthProvider {
    fn id(&self) -> AuthProviderId {
        AuthProviderId::Bootstrap
    }

    async fn resolve(
        &self,
        headers: &HeaderMap,
        state: &AppState,
    ) -> Result<AuthContext, ApiError> {
        if let Some(token) = extract_bearer(headers) {
            return validate_clotho_token(state, &token).await;
        }
        if !state.auth_required {
            return Ok(AuthContext::from_bootstrap(&state.bootstrap));
        }
        Err(ApiError::Unauthorized(
            "authentication required; send Authorization: Bearer <token>".into(),
        ))
    }
}
