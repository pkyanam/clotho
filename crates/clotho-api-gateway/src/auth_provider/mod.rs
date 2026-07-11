//! Pluggable human AuthProvider (ADR-0018 / Stage 17).
//!
//! Clotho services depend on this boundary + Postgres mappings — never on
//! Clerk SDK types leaking into VCS, storage, or MCP crates. Agents stay on
//! ADR-0005 (`clotho_agt_…`) and are never modeled as Clerk users.

mod bootstrap;
mod clerk;

use std::sync::Arc;

use async_trait::async_trait;
use axum::http::HeaderMap;

use crate::auth::AuthContext;
use crate::error::ApiError;
use crate::AppState;

pub use bootstrap::BootstrapAuthProvider;
pub use clerk::{ClerkAuthProvider, ClerkConfig};

/// Which human AuthProvider is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthProviderId {
    Bootstrap,
    Clerk,
}

impl AuthProviderId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Clerk => "clerk",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, ApiError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "bootstrap" => Ok(Self::Bootstrap),
            "clerk" => Ok(Self::Clerk),
            other => Err(ApiError::Internal(format!(
                "unknown CLOTHO_AUTH_PROVIDER {other:?}; expected bootstrap|clerk"
            ))),
        }
    }
}

/// Human authentication boundary (sessions + API keys).
#[async_trait]
pub(crate) trait AuthProvider: Send + Sync {
    fn id(&self) -> AuthProviderId;

    /// Resolve a request into a Clotho human principal.
    async fn resolve(&self, headers: &HeaderMap, state: &AppState)
        -> Result<AuthContext, ApiError>;
}

/// Build the configured AuthProvider from gateway config / env.
pub(crate) fn build_auth_provider(
    id: AuthProviderId,
    clerk: Option<ClerkConfig>,
) -> Result<Arc<dyn AuthProvider>, ApiError> {
    match id {
        AuthProviderId::Bootstrap => Ok(Arc::new(BootstrapAuthProvider)),
        AuthProviderId::Clerk => {
            let cfg = clerk.ok_or_else(|| {
                ApiError::Internal(
                    "CLOTHO_AUTH_PROVIDER=clerk requires Clerk config (CLERK_SECRET_KEY or CLOTHO_CLERK_JWT_SECRET)".into(),
                )
            })?;
            Ok(Arc::new(ClerkAuthProvider::new(cfg)))
        }
    }
}

pub fn auth_provider_from_env() -> Result<(AuthProviderId, Option<ClerkConfig>), ApiError> {
    let raw = std::env::var("CLOTHO_AUTH_PROVIDER").unwrap_or_else(|_| "bootstrap".into());
    let id = AuthProviderId::parse(&raw)?;
    let clerk = match id {
        AuthProviderId::Bootstrap => ClerkConfig::from_env().ok(),
        AuthProviderId::Clerk => Some(ClerkConfig::from_env()?),
    };
    Ok((id, clerk))
}
