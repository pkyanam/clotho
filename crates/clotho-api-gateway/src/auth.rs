//! Human API tokens and request authentication (Slice A, ADR-0015).
//!
//! Stage 17 routes resolution through [`crate::auth_provider::AuthProvider`]
//! (bootstrap | clerk). Token mint/list/revoke remain Clotho-owned (§11 #7).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::control::Bootstrap;
use crate::error::ApiError;
use crate::AppState;

pub const TOKEN_PREFIX: &str = "clotho_tok_";

/// Authenticated caller resolved for every request.
#[derive(Clone, Debug)]
pub struct AuthContext {
    pub user_id: String,
    pub user_name: String,
    pub token_id: Option<String>,
}

impl AuthContext {
    pub fn from_bootstrap(b: &Bootstrap) -> Self {
        Self {
            user_id: b.user_id.clone(),
            user_name: b.user_name.clone(),
            token_id: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct UserPublic {
    pub id: String,
    pub name: String,
    pub email: String,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MeResponse {
    pub user: UserPublic,
    pub token_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct TokenMeta {
    pub id: String,
    pub name: String,
    pub token_prefix: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TokenListResponse {
    pub tokens: Vec<TokenMeta>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateTokenRequest {
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CreateTokenResponse {
    pub id: String,
    pub name: String,
    pub token: String,
    pub token_prefix: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
}

pub fn hash_token(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn mint_plaintext_token() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{TOKEN_PREFIX}{hex}")
}

fn display_prefix(plaintext: &str) -> String {
    plaintext.chars().take(12).collect()
}

pub(crate) fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(axum::http::header::AUTHORIZATION)?;
    let s = value.to_str().ok()?;
    let rest = s.strip_prefix("Bearer ")?.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

/// Validate a Clotho-minted `clotho_tok_…` human API token.
pub(crate) async fn validate_clotho_token(
    state: &AppState,
    plaintext: &str,
) -> Result<AuthContext, ApiError> {
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("database is not configured".into()))?;
    let hash = hash_token(plaintext);
    let row = sqlx::query(
        r#"
        select t.id, t.user_id, u.name as user_name
        from api_tokens t
        join users u on u.id = t.user_id
        where t.token_hash = $1
          and t.revoked_at is null
          and (t.expires_at is null or t.expires_at > now())
        "#,
    )
    .bind(&hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("validate token: {e}")))?;
    let Some(row) = row else {
        return Err(ApiError::Unauthorized("invalid or expired token".into()));
    };
    let token_id: String = row.get("id");
    let user_id: String = row.get("user_id");
    let user_name: String = row.get("user_name");
    let _ = sqlx::query("update api_tokens set last_used_at = now() where id = $1")
        .bind(&token_id)
        .execute(pool)
        .await;
    Ok(AuthContext {
        user_id,
        user_name,
        token_id: Some(token_id),
    })
}

/// Resolve the actor for a request via the configured AuthProvider.
pub(crate) async fn resolve_auth(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<AuthContext, ApiError> {
    state.auth_provider.resolve(headers, state).await
}

/// Provision the explicitly configured bootstrap token, if any.
///
/// Local development defaults to open bootstrap auth, so an operator can mint
/// a token deliberately with `clotho auth token create`. Required-auth
/// deployments must provide `CLOTHO_BOOTSTRAP_TOKEN` through their secret
/// manager before startup. Bootstrap credential plaintext is never logged.
pub async fn ensure_bootstrap_token(pool: &PgPool, user_id: &str) -> Result<(), ApiError> {
    let Some(plaintext) = configured_bootstrap_token(std::env::var("CLOTHO_BOOTSTRAP_TOKEN").ok())
    else {
        tracing::info!(
            "bootstrap API token not auto-minted; local open auth can mint one with `clotho auth token create`, and required-auth deployments must set CLOTHO_BOOTSTRAP_TOKEN before startup"
        );
        return Ok(());
    };
    let id = Uuid::new_v4().to_string();
    let prefix = display_prefix(&plaintext);
    let hash = hash_token(&plaintext);

    sqlx::query(
        r#"
        insert into api_tokens (id, user_id, name, token_hash, token_prefix, scopes)
        values ($1, $2, 'bootstrap', $3, $4, '{*}')
        on conflict (token_hash) do update set
          revoked_at = null,
          user_id = excluded.user_id,
          token_prefix = excluded.token_prefix
        "#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(&hash)
    .bind(&prefix)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("bootstrap token insert: {e}")))?;

    tracing::info!(
        "bootstrap API token provisioned from CLOTHO_BOOTSTRAP_TOKEN; credential plaintext is not logged"
    );
    Ok(())
}

fn configured_bootstrap_token(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

async fn get_user(pool: &PgPool, user_id: &str) -> Result<UserPublic, ApiError> {
    sqlx::query_as::<_, UserPublic>("select * from users where id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("get user: {e}")))?
        .ok_or_else(|| ApiError::NotFound("user not found".into()))
}

pub(crate) async fn me_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("database is not configured".into()))?;
    let user = get_user(pool, &auth.user_id).await?;
    Ok(Json(MeResponse {
        user,
        token_id: auth.token_id,
    }))
}

pub(crate) async fn list_tokens_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<TokenListResponse>, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("database is not configured".into()))?;
    let tokens = sqlx::query_as::<_, TokenMeta>(
        r#"
        select id, name, token_prefix, scopes, created_at, last_used_at, expires_at
        from api_tokens
        where user_id = $1 and revoked_at is null
        order by created_at desc
        "#,
    )
    .bind(&auth.user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("list tokens: {e}")))?;
    Ok(Json(TokenListResponse { tokens }))
}

pub(crate) async fn create_token_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateTokenRequest>,
) -> Result<(StatusCode, Json<CreateTokenResponse>), ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("database is not configured".into()))?;
    let plaintext = mint_plaintext_token();
    let id = Uuid::new_v4().to_string();
    let prefix = display_prefix(&plaintext);
    let hash = hash_token(&plaintext);
    let name = if req.name.trim().is_empty() {
        "api token".into()
    } else {
        req.name.trim().into()
    };

    let row = sqlx::query(
        r#"
        insert into api_tokens (id, user_id, name, token_hash, token_prefix, scopes)
        values ($1, $2, $3, $4, $5, '{*}')
        returning created_at
        "#,
    )
    .bind(&id)
    .bind(&auth.user_id)
    .bind(&name)
    .bind(&hash)
    .bind(&prefix)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("create token: {e}")))?;

    let created_at: DateTime<Utc> = row.get("created_at");
    Ok((
        StatusCode::CREATED,
        Json(CreateTokenResponse {
            id,
            name,
            token: plaintext,
            token_prefix: prefix,
            scopes: vec!["*".into()],
            created_at,
        }),
    ))
}

pub(crate) async fn revoke_token_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let auth = resolve_auth(&headers, &state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("database is not configured".into()))?;
    let result = sqlx::query(
        "update api_tokens set revoked_at = now() where id = $1 and user_id = $2 and revoked_at is null",
    )
    .bind(&id)
    .bind(&auth.user_id)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("revoke token: {e}")))?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("token {id:?} not found")));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_token_is_deterministic() {
        let h1 = hash_token("clotho_tok_abc");
        let h2 = hash_token("clotho_tok_abc");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn minted_token_has_prefix() {
        let t = mint_plaintext_token();
        assert!(t.starts_with(TOKEN_PREFIX));
        assert!(t.len() > TOKEN_PREFIX.len());
    }

    #[test]
    fn bootstrap_token_requires_explicit_nonempty_configuration() {
        assert_eq!(configured_bootstrap_token(None), None);
        assert_eq!(configured_bootstrap_token(Some("   ".into())), None);
        assert_eq!(
            configured_bootstrap_token(Some("  clotho_tok_configured  ".into())),
            Some("clotho_tok_configured".into())
        );
    }
}
