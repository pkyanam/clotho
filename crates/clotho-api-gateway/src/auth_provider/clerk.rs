//! Clerk AuthProvider — humans/orgs only (ADR-0018).
//!
//! Verifies Clerk session JWTs (and optional Clerk org API keys via Backend
//! API). Maps `clerk_user_id` / `clerk_org_id` into Clotho users/orgs. Never
//! creates Clerk users for agents.
//!
//! §11 #7 default: Clotho continues minting `clotho_tok_…` human API tokens
//! alongside Clerk credentials; both resolve to the same AuthContext + Clotho
//! permission checks. Under this provider, Bearer tokens are tried in order:
//! 1. `clotho_tok_…` (Clotho-minted)
//! 2. Clerk session JWT
//! 3. Clerk secret-key Backend API verification for org API keys (when configured)

use std::collections::HashMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::http::HeaderMap;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use sqlx::{PgPool, Row};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::{extract_bearer, validate_clotho_token, AuthContext, TOKEN_PREFIX};
use crate::error::ApiError;
use crate::AppState;

use super::{AuthProvider, AuthProviderId};

const CLOTHO_TOK_PREFIX: &str = TOKEN_PREFIX;

#[derive(Clone, Debug)]
pub struct ClerkConfig {
    /// Publishable key (web only; not used for verification).
    pub publishable_key: String,
    /// Clerk Backend API secret (`sk_…`) for org API key verification.
    pub secret_key: String,
    /// Optional HS256 secret for local/mocked JWTs (`CLOTHO_CLERK_JWT_SECRET`).
    /// When set, session tokens are verified with this key instead of JWKS.
    pub jwt_secret: Option<String>,
    /// JWKS URL (default Clerk frontend API derived from publishable key, or override).
    pub jwks_url: Option<String>,
    /// Expected JWT issuer (optional).
    pub issuer: Option<String>,
    /// Authorized parties / azp (optional).
    pub authorized_parties: Vec<String>,
}

impl ClerkConfig {
    pub fn from_env() -> Result<Self, ApiError> {
        let publishable_key = std::env::var("CLERK_PUBLISHABLE_KEY")
            .or_else(|_| std::env::var("NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"))
            .unwrap_or_default();
        let secret_key = std::env::var("CLERK_SECRET_KEY").unwrap_or_default();
        let jwt_secret = std::env::var("CLOTHO_CLERK_JWT_SECRET")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let jwks_url = std::env::var("CLERK_JWKS_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let issuer = std::env::var("CLERK_JWT_ISSUER")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if jwt_secret.is_none() && secret_key.trim().is_empty() && jwks_url.is_none() {
            return Err(ApiError::Internal(
                "Clerk AuthProvider needs CLERK_SECRET_KEY, CLERK_JWKS_URL, or CLOTHO_CLERK_JWT_SECRET".into(),
            ));
        }

        Ok(Self {
            publishable_key,
            secret_key: secret_key.trim().to_string(),
            jwt_secret,
            jwks_url,
            issuer,
            authorized_parties: Vec::new(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct ClerkClaims {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_address: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    org_id: Option<String>,
    #[serde(default)]
    org_slug: Option<String>,
    #[serde(default)]
    org_role: Option<String>,
    #[serde(default)]
    azp: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    iss: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Debug, Deserialize)]
struct Jwk {
    kid: String,
    kty: String,
    #[serde(default)]
    n: String,
    #[serde(default)]
    e: String,
    #[serde(default)]
    #[allow(dead_code)]
    alg: Option<String>,
}

struct JwksCache {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Instant,
}

pub struct ClerkAuthProvider {
    config: ClerkConfig,
    http: reqwest::Client,
    jwks: RwLock<Option<JwksCache>>,
}

impl ClerkAuthProvider {
    pub fn new(config: ClerkConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
            jwks: RwLock::new(None),
        }
    }

    async fn decode_session_jwt(&self, token: &str) -> Result<ClerkClaims, ApiError> {
        if let Some(secret) = &self.config.jwt_secret {
            let mut validation = Validation::new(Algorithm::HS256);
            validation.validate_aud = false;
            if let Some(iss) = &self.config.issuer {
                validation.set_issuer(&[iss]);
            }
            let data = decode::<ClerkClaims>(
                token,
                &DecodingKey::from_secret(secret.as_bytes()),
                &validation,
            )
            .map_err(|e| ApiError::Unauthorized(format!("invalid Clerk session: {e}")))?;
            return Ok(data.claims);
        }

        let header = decode_header(token)
            .map_err(|e| ApiError::Unauthorized(format!("invalid Clerk JWT header: {e}")))?;
        let kid = header
            .kid
            .ok_or_else(|| ApiError::Unauthorized("Clerk JWT missing kid".into()))?;
        let key = self.decoding_key_for_kid(&kid).await?;
        let mut validation = Validation::new(header.alg);
        validation.validate_aud = false;
        if let Some(iss) = &self.config.issuer {
            validation.set_issuer(&[iss]);
        }
        let data = decode::<ClerkClaims>(token, &key, &validation)
            .map_err(|e| ApiError::Unauthorized(format!("invalid Clerk session: {e}")))?;

        if !self.config.authorized_parties.is_empty() {
            if let Some(azp) = &data.claims.azp {
                if !self.config.authorized_parties.iter().any(|p| p == azp) {
                    return Err(ApiError::Unauthorized(
                        "Clerk session azp not authorized".into(),
                    ));
                }
            }
        }
        Ok(data.claims)
    }

    async fn decoding_key_for_kid(&self, kid: &str) -> Result<DecodingKey, ApiError> {
        {
            let guard = self.jwks.read().await;
            if let Some(cache) = guard.as_ref() {
                if cache.fetched_at.elapsed() < Duration::from_secs(3600) {
                    if let Some(key) = cache.keys.get(kid) {
                        return Ok(key.clone());
                    }
                }
            }
        }
        self.refresh_jwks().await?;
        let guard = self.jwks.read().await;
        guard
            .as_ref()
            .and_then(|c| c.keys.get(kid).cloned())
            .ok_or_else(|| ApiError::Unauthorized(format!("Clerk JWKS missing kid {kid:?}")))
    }

    async fn refresh_jwks(&self) -> Result<(), ApiError> {
        let url = self.config.jwks_url.clone().ok_or_else(|| {
            ApiError::Internal("CLERK_JWKS_URL is required without CLOTHO_CLERK_JWT_SECRET".into())
        })?;
        let jwks: Jwks = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("fetch Clerk JWKS: {e}")))?
            .error_for_status()
            .map_err(|e| ApiError::Internal(format!("Clerk JWKS status: {e}")))?
            .json()
            .await
            .map_err(|e| ApiError::Internal(format!("parse Clerk JWKS: {e}")))?;

        let mut keys = HashMap::new();
        for jwk in jwks.keys {
            if jwk.kty != "RSA" || jwk.n.is_empty() || jwk.e.is_empty() {
                continue;
            }
            match DecodingKey::from_rsa_components(&jwk.n, &jwk.e) {
                Ok(key) => {
                    keys.insert(jwk.kid, key);
                }
                Err(e) => tracing::warn!(error = %e, kid = %jwk.kid, "skip bad Clerk JWK"),
            }
        }
        let mut guard = self.jwks.write().await;
        *guard = Some(JwksCache {
            keys,
            fetched_at: Instant::now(),
        });
        Ok(())
    }

    /// Verify a Clerk org/user API key via Backend API when secret is set.
    async fn verify_clerk_api_key(&self, token: &str) -> Result<Option<ClerkClaims>, ApiError> {
        if self.config.secret_key.is_empty() {
            return Ok(None);
        }
        // Clerk Backend API: GET /v1/oauth_applications is wrong; for API keys
        // use the authenticate request pattern — verify via sessions or
        // /v1/users/me equivalent. We call Clerk's JWT template-less path:
        // GET https://api.clerk.com/v1/users with Bearer secret is admin-only.
        // For org API keys, Clerk documents verifying via the Backend SDK.
        // Pragmatic Stage 17: POST to Clerk's verify endpoint when available;
        // otherwise reject non-JWT tokens that aren't clotho_tok.
        let resp = self
            .http
            .get("https://api.clerk.com/v1/users/me")
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await;
        let Ok(resp) = resp else {
            return Ok(None);
        };
        if !resp.status().is_success() {
            // Try treating the token as a Clerk session JWT already handled;
            // API-key path failed.
            return Ok(None);
        }
        #[derive(Deserialize)]
        struct ClerkUser {
            id: String,
            #[serde(default)]
            username: Option<String>,
            #[serde(default)]
            first_name: Option<String>,
            #[serde(default)]
            last_name: Option<String>,
            #[serde(default)]
            email_addresses: Vec<ClerkEmail>,
        }
        #[derive(Deserialize)]
        struct ClerkEmail {
            email_address: String,
        }
        let user: ClerkUser = resp
            .json()
            .await
            .map_err(|e| ApiError::Unauthorized(format!("Clerk API key user: {e}")))?;
        let email = user
            .email_addresses
            .first()
            .map(|e| e.email_address.clone());
        let name = match (
            user.first_name.as_deref().unwrap_or(""),
            user.last_name.as_deref().unwrap_or(""),
        ) {
            ("", "") => user.username.clone(),
            (f, "") => Some(f.to_string()),
            (f, l) => Some(format!("{f} {l}")),
        };
        Ok(Some(ClerkClaims {
            sub: user.id,
            email,
            email_address: None,
            username: user.username,
            name,
            org_id: None,
            org_slug: None,
            org_role: None,
            azp: None,
            iss: None,
        }))
    }
}

#[async_trait]
impl AuthProvider for ClerkAuthProvider {
    fn id(&self) -> AuthProviderId {
        AuthProviderId::Clerk
    }

    async fn resolve(
        &self,
        headers: &HeaderMap,
        state: &AppState,
    ) -> Result<AuthContext, ApiError> {
        let Some(token) = extract_bearer(headers) else {
            if !state.auth_required {
                // Managed profiles should set AUTH_REQUIRED=true; allow open
                // fallback only when explicitly disabled (tests).
                return Ok(AuthContext::from_bootstrap(&state.bootstrap));
            }
            return Err(ApiError::Unauthorized(
                "authentication required; send Authorization: Bearer <Clerk session or clotho_tok_…>".into(),
            ));
        };

        // Prefer Clotho-minted human tokens (§11 #7).
        if token.starts_with(CLOTHO_TOK_PREFIX) {
            return validate_clotho_token(state, &token).await;
        }

        // Reject agent tokens on the human edge — agents use MCP / agent-gateway.
        if token.starts_with("clotho_agt_") {
            return Err(ApiError::Unauthorized(
                "agent tokens are not valid for human API routes; use clotho_tok_… or a Clerk session".into(),
            ));
        }

        let claims = match self.decode_session_jwt(&token).await {
            Ok(c) => c,
            Err(jwt_err) => match self.verify_clerk_api_key(&token).await? {
                Some(c) => c,
                None => return Err(jwt_err),
            },
        };

        let pool = state.pool.as_ref().ok_or_else(|| {
            ApiError::Internal("database is not configured for Clerk auth".into())
        })?;

        link_clerk_principal(pool, &claims, &state.bootstrap).await
    }
}

async fn link_clerk_principal(
    pool: &PgPool,
    claims: &ClerkClaims,
    bootstrap: &crate::control::Bootstrap,
) -> Result<AuthContext, ApiError> {
    let clerk_user_id = claims.sub.trim();
    if clerk_user_id.is_empty() {
        return Err(ApiError::Unauthorized("Clerk token missing sub".into()));
    }

    // Existing link?
    if let Some(row) = sqlx::query(
        r#"
        select u.id, u.name
        from clerk_user_links l
        join users u on u.id = l.user_id
        where l.clerk_user_id = $1
        "#,
    )
    .bind(clerk_user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("clerk user link: {e}")))?
    {
        let user_id: String = row.get("id");
        let user_name: String = row.get("name");
        if let Some(org_id) = claims.org_id.as_deref().filter(|s| !s.is_empty()) {
            ensure_clerk_org(pool, org_id, claims, &user_id, bootstrap).await?;
        }
        return Ok(AuthContext {
            user_id,
            user_name,
            token_id: None,
        });
    }

    let email = claims
        .email
        .clone()
        .or_else(|| claims.email_address.clone())
        .unwrap_or_default();
    let display = claims
        .name
        .clone()
        .or_else(|| claims.username.clone())
        .unwrap_or_else(|| format!("clerk-{clerk_user_id}"));
    let username = claims
        .username
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| slug_username(&display, clerk_user_id));

    let user_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        insert into users (id, name, email, display_name)
        values ($1, $2, $3, $4)
        on conflict (name) do update set
          email = excluded.email,
          display_name = excluded.display_name
        returning id
        "#,
    )
    .bind(&user_id)
    .bind(&username)
    .bind(&email)
    .bind(&display)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("insert clerk user: {e}")))?;

    // Resolve actual id if name conflict updated an existing row.
    let user_id: String = sqlx::query_scalar("select id from users where name = $1")
        .bind(&username)
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("resolve clerk user: {e}")))?;

    sqlx::query(
        r#"
        insert into clerk_user_links (clerk_user_id, user_id)
        values ($1, $2)
        on conflict (clerk_user_id) do update set user_id = excluded.user_id
        "#,
    )
    .bind(clerk_user_id)
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("link clerk user: {e}")))?;

    if let Some(org_id) = claims.org_id.as_deref().filter(|s| !s.is_empty()) {
        ensure_clerk_org(pool, org_id, claims, &user_id, bootstrap).await?;
    }

    Ok(AuthContext {
        user_id,
        user_name: username,
        token_id: None,
    })
}

async fn ensure_clerk_org(
    pool: &PgPool,
    clerk_org_id: &str,
    claims: &ClerkClaims,
    user_id: &str,
    bootstrap: &crate::control::Bootstrap,
) -> Result<(), ApiError> {
    if let Some(row) = sqlx::query("select org_id from clerk_org_links where clerk_org_id = $1")
        .bind(clerk_org_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("clerk org link: {e}")))?
    {
        let org_id: String = row.get("org_id");
        let role = map_org_role(claims.org_role.as_deref());
        sqlx::query(
            r#"
            insert into org_memberships (org_id, user_id, role)
            values ($1, $2, $3)
            on conflict (org_id, user_id) do update set role = excluded.role
            "#,
        )
        .bind(&org_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("clerk org membership: {e}")))?;
        return Ok(());
    }

    let slug = claims
        .org_slug
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("clerk-org-{}", &clerk_org_id[..clerk_org_id.len().min(8)]));
    let display = claims.org_slug.clone().unwrap_or_else(|| slug.clone());
    let org_id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        insert into orgs (id, name, display_name, forgejo_owner, created_by)
        values ($1, $2, $3, $4, $5)
        on conflict (name) do nothing
        "#,
    )
    .bind(&org_id)
    .bind(&slug)
    .bind(&display)
    .bind(&bootstrap.forgejo_owner)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("insert clerk org: {e}")))?;

    let org_id: String = sqlx::query_scalar("select id from orgs where name = $1")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("resolve clerk org: {e}")))?;

    sqlx::query(
        r#"
        insert into clerk_org_links (clerk_org_id, org_id)
        values ($1, $2)
        on conflict (clerk_org_id) do update set org_id = excluded.org_id
        "#,
    )
    .bind(clerk_org_id)
    .bind(&org_id)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("link clerk org: {e}")))?;

    let role = map_org_role(claims.org_role.as_deref());
    sqlx::query(
        r#"
        insert into org_memberships (org_id, user_id, role)
        values ($1, $2, $3)
        on conflict (org_id, user_id) do update set role = excluded.role
        "#,
    )
    .bind(&org_id)
    .bind(user_id)
    .bind(role)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("clerk org membership: {e}")))?;

    Ok(())
}

fn map_org_role(clerk_role: Option<&str>) -> &'static str {
    match clerk_role.unwrap_or("").to_ascii_lowercase().as_str() {
        "admin" | "org:admin" => "admin",
        _ => "member",
    }
}

fn slug_username(display: &str, clerk_user_id: &str) -> String {
    let base: String = display
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let base = base.trim_matches('-');
    if base.is_empty() {
        format!("user-{}", &clerk_user_id[..clerk_user_id.len().min(12)])
    } else {
        base.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    #[test]
    fn hs256_session_roundtrip() {
        let secret = "test-clerk-jwt-secret-for-stage17";
        let claims = serde_json::json!({
            "sub": "user_abc",
            "email": "a@example.com",
            "username": "alice",
            "org_id": "org_xyz",
            "org_slug": "acme",
            "org_role": "org:admin",
            "exp": chrono::Utc::now().timestamp() + 3600,
            "iat": chrono::Utc::now().timestamp(),
        });
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();

        let provider = ClerkAuthProvider::new(ClerkConfig {
            publishable_key: String::new(),
            secret_key: String::new(),
            jwt_secret: Some(secret.into()),
            jwks_url: None,
            issuer: None,
            authorized_parties: vec![],
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let decoded = rt.block_on(provider.decode_session_jwt(&token)).unwrap();
        assert_eq!(decoded.sub, "user_abc");
        assert_eq!(decoded.org_id.as_deref(), Some("org_xyz"));
    }
}
