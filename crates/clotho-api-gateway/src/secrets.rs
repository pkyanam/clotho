//! Clotho secrets store (Stage 13, docs/adr/0014).
//!
//! Org- and repo-scoped secrets sealed with AES-256-GCM. API responses never
//! return plaintext — only metadata and an optional last-4 mask.

use std::sync::Arc;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth;
use crate::control::{self, ActivityEventInput};
use crate::error::ApiError;
use crate::AppState;

const NONCE_LEN: usize = 12;

/// Well-known secret names that bind to direct CCI providers.
pub const SECRET_DAYTONA_API_KEY: &str = "DAYTONA_API_KEY";
pub const SECRET_BOX_API_KEY: &str = "BOX_API_KEY";

use crate::computesdk_catalog;

/// Master key for sealing secret values. Loaded from CLOTHO_SECRETS_MASTER_KEY.
#[derive(Clone)]
pub struct SecretsCrypto {
    cipher: Aes256Gcm,
}

impl SecretsCrypto {
    /// Parse a 32-byte key from base64 or 64-char hex. Returns None when unset.
    pub fn from_env() -> Result<Option<Self>, String> {
        let raw = match std::env::var("CLOTHO_SECRETS_MASTER_KEY") {
            Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => return Ok(None),
        };
        let key = decode_master_key(&raw)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| format!("CLOTHO_SECRETS_MASTER_KEY invalid: {e}"))?;
        Ok(Some(Self { cipher }))
    }

    pub fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, ApiError> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        // Os entropy for GCM nonces; unique per seal.
        getrandom_fill(&mut nonce_bytes)?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| ApiError::Internal(format!("encrypt secret: {e}")))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    pub fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, ApiError> {
        if sealed.len() < NONCE_LEN + 16 {
            return Err(ApiError::Internal("corrupt secret ciphertext".into()));
        }
        let (nonce_bytes, ct) = sealed.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        self.cipher
            .decrypt(nonce, ct)
            .map_err(|e| ApiError::Internal(format!("decrypt secret: {e}")))
    }
}

fn getrandom_fill(buf: &mut [u8]) -> Result<(), ApiError> {
    use rand::Rng;
    rand::rng().fill_bytes(buf);
    Ok(())
}

fn decode_master_key(raw: &str) -> Result<[u8; 32], String> {
    if raw.len() == 64 && raw.chars().all(|c| c.is_ascii_hexdigit()) {
        let mut out = [0u8; 32];
        for i in 0..32 {
            out[i] = u8::from_str_radix(&raw[i * 2..i * 2 + 2], 16)
                .map_err(|e| format!("CLOTHO_SECRETS_MASTER_KEY hex: {e}"))?;
        }
        return Ok(out);
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(raw.as_bytes())
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(raw.as_bytes()))
        .map_err(|e| format!("CLOTHO_SECRETS_MASTER_KEY base64: {e}"))?;
    if decoded.len() != 32 {
        return Err(format!(
            "CLOTHO_SECRETS_MASTER_KEY must decode to 32 bytes, got {}",
            decoded.len()
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&decoded);
    Ok(out)
}

fn require_crypto(state: &AppState) -> Result<&SecretsCrypto, ApiError> {
    state.secrets_crypto.as_ref().ok_or_else(|| {
        ApiError::InvalidRequest(
            "secrets master key is not configured — set CLOTHO_SECRETS_MASTER_KEY on the api gateway"
                .into(),
        )
    })
}

fn require_pool(state: &AppState) -> Result<&PgPool, ApiError> {
    state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("control-plane database is not configured".into()))
}

/// Metadata-only secret view (never includes plaintext).
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct SecretMeta {
    pub id: String,
    pub scope: String,
    pub org_id: Option<String>,
    pub repo_id: Option<String>,
    pub name: String,
    pub description: String,
    pub value_last4: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct SecretListResponse {
    pub secrets: Vec<SecretMeta>,
}

#[derive(Deserialize)]
pub struct UpsertSecretRequest {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Deserialize)]
pub struct PatchSecretRequest {
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct ConnectProviderRequest {
    /// Provider API key (write-once). For single-key ComputeSDK upstreams this
    /// is the sole credential when `credentials` is empty.
    #[serde(default)]
    pub api_key: String,
    /// Org that owns the secret; defaults to bootstrap org.
    #[serde(default)]
    pub org: String,
    /// ComputeSDK upstream id (`e2b`, `vercel`, `modal`, …). See catalog.
    #[serde(default)]
    pub upstream: String,
    /// Multi-field credentials keyed by env/secret name (e.g. VERCEL_TOKEN).
    /// Preferred for multi-key upstreams; values never returned after write.
    #[serde(default)]
    pub credentials: std::collections::HashMap<String, String>,
    /// Deprecated aliases kept for older clients (Modal).
    #[serde(default)]
    pub modal_token_id: String,
    #[serde(default)]
    pub modal_token_secret: String,
}

#[derive(Serialize)]
pub struct ComputesdkUpstreamsResponse {
    pub upstreams: Vec<computesdk_catalog::ComputesdkUpstream>,
}

#[derive(Serialize)]
pub struct DisconnectProviderResponse {
    pub provider: String,
    pub deleted_secrets: Vec<String>,
}

fn last4(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let n = chars.len().min(4);
    chars[chars.len() - n..].iter().collect()
}

fn valid_secret_name(name: &str) -> Result<(), ApiError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ApiError::InvalidRequest("secret name is required".into()));
    }
    if name.len() > 128 {
        return Err(ApiError::InvalidRequest("secret name is too long".into()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ApiError::InvalidRequest(
            "secret name must be letters, digits, '_' or '-'".into(),
        ));
    }
    Ok(())
}

/// Routes mounted under the gateway router.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/orgs/{org}/secrets",
            get(list_org_secrets).post(create_org_secret),
        )
        .route(
            "/api/v1/orgs/{org}/secrets/{name}",
            get(get_org_secret)
                .patch(patch_org_secret)
                .delete(delete_org_secret),
        )
        .route(
            "/api/v1/repos/{repo}/secrets",
            get(list_repo_secrets).post(create_repo_secret),
        )
        .route(
            "/api/v1/repos/{repo}/secrets/{name}",
            get(get_repo_secret)
                .patch(patch_repo_secret)
                .delete(delete_repo_secret),
        )
        .route(
            "/api/v1/providers/{provider}/connect",
            post(connect_provider).delete(disconnect_provider),
        )
        .route(
            "/api/v1/providers/computesdk/upstreams",
            get(list_computesdk_upstreams),
        )
}

async fn list_computesdk_upstreams() -> Json<ComputesdkUpstreamsResponse> {
    Json(ComputesdkUpstreamsResponse {
        upstreams: computesdk_catalog::UPSTREAMS.to_vec(),
    })
}

// ---------------------------------------------------------------------------
// handlers — org scope
// ---------------------------------------------------------------------------

async fn list_org_secrets(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<SecretListResponse>, ApiError> {
    let pool = require_pool(&state)?;
    let org_row = control::get_org(pool, &org).await?;
    let secrets = list_by_org(pool, &org_row.id).await?;
    Ok(Json(SecretListResponse { secrets }))
}

async fn create_org_secret(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(org): Path<String>,
    Json(body): Json<UpsertSecretRequest>,
) -> Result<(StatusCode, Json<SecretMeta>), ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = require_pool(&state)?;
    let crypto = require_crypto(&state)?;
    let org_row = control::get_org(pool, &org).await?;
    control::require_org_role(pool, &org_row.id, &auth.user_id, "admin").await?;
    valid_secret_name(&body.name)?;
    if body.value.is_empty() {
        return Err(ApiError::InvalidRequest("secret value is required".into()));
    }
    let name = body.name.trim().to_string();
    let meta = upsert_org_secret(
        pool,
        crypto,
        &org_row.id,
        &name,
        &body.value,
        body.description.trim(),
        &auth.user_id,
        true,
    )
    .await?;
    control::log_activity(
        pool,
        ActivityEventInput {
            actor_id: auth.user_id.clone(),
            org_id: Some(org_row.id),
            repo_id: None,
            event_type: "secret.created".into(),
            payload: serde_json::json!({"scope": "org", "name": name}),
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(meta)))
}

async fn get_org_secret(
    State(state): State<Arc<AppState>>,
    Path((org, name)): Path<(String, String)>,
) -> Result<Json<SecretMeta>, ApiError> {
    let pool = require_pool(&state)?;
    let org_row = control::get_org(pool, &org).await?;
    let meta = get_by_org_name(pool, &org_row.id, &name).await?;
    Ok(Json(meta))
}

async fn patch_org_secret(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((org, name)): Path<(String, String)>,
    Json(body): Json<PatchSecretRequest>,
) -> Result<Json<SecretMeta>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = require_pool(&state)?;
    let crypto = require_crypto(&state)?;
    let org_row = control::get_org(pool, &org).await?;
    control::require_org_role(pool, &org_row.id, &auth.user_id, "admin").await?;
    let existing = get_by_org_name(pool, &org_row.id, &name).await?;
    let description = body
        .description
        .as_deref()
        .map(str::trim)
        .unwrap_or(existing.description.as_str());
    let value = match &body.value {
        Some(v) if !v.is_empty() => v.as_str(),
        Some(_) => {
            return Err(ApiError::InvalidRequest(
                "secret value cannot be empty; omit value to keep current".into(),
            ))
        }
        None => "",
    };
    let meta = if value.is_empty() {
        // Description-only update; keep ciphertext.
        update_description(pool, &existing.id, description).await?
    } else {
        upsert_org_secret(
            pool,
            crypto,
            &org_row.id,
            &name,
            value,
            description,
            &auth.user_id,
            false,
        )
        .await?
    };
    control::log_activity(
        pool,
        ActivityEventInput {
            actor_id: auth.user_id.clone(),
            org_id: Some(org_row.id),
            repo_id: None,
            event_type: "secret.updated".into(),
            payload: serde_json::json!({"scope": "org", "name": name}),
        },
    )
    .await?;
    Ok(Json(meta))
}

async fn delete_org_secret(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((org, name)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = require_pool(&state)?;
    let org_row = control::get_org(pool, &org).await?;
    control::require_org_role(pool, &org_row.id, &auth.user_id, "admin").await?;
    let deleted = delete_by_org_name(pool, &org_row.id, &name).await?;
    if !deleted {
        return Err(ApiError::NotFound(format!("secret {name:?} not found")));
    }
    control::log_activity(
        pool,
        ActivityEventInput {
            actor_id: auth.user_id.clone(),
            org_id: Some(org_row.id),
            repo_id: None,
            event_type: "secret.deleted".into(),
            payload: serde_json::json!({"scope": "org", "name": name}),
        },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// handlers — repo scope
// ---------------------------------------------------------------------------

async fn list_repo_secrets(
    State(state): State<Arc<AppState>>,
    Path(repo): Path<String>,
) -> Result<Json<SecretListResponse>, ApiError> {
    let pool = require_pool(&state)?;
    let repo_row = control::get_repo_by_name(pool, &repo).await?;
    let secrets = list_by_repo(pool, &repo_row.id).await?;
    Ok(Json(SecretListResponse { secrets }))
}

async fn create_repo_secret(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(repo): Path<String>,
    Json(body): Json<UpsertSecretRequest>,
) -> Result<(StatusCode, Json<SecretMeta>), ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = require_pool(&state)?;
    let crypto = require_crypto(&state)?;
    let clotho = control::require_repo_admin(pool, &repo, &auth.user_id).await?;
    valid_secret_name(&body.name)?;
    if body.value.is_empty() {
        return Err(ApiError::InvalidRequest("secret value is required".into()));
    }
    let name = body.name.trim().to_string();
    let meta = upsert_repo_secret(
        pool,
        crypto,
        &clotho.repo.org_id,
        &clotho.repo.id,
        &name,
        &body.value,
        body.description.trim(),
        &auth.user_id,
        true,
    )
    .await?;
    control::log_activity(
        pool,
        ActivityEventInput {
            actor_id: auth.user_id.clone(),
            org_id: Some(clotho.repo.org_id),
            repo_id: Some(clotho.repo.id),
            event_type: "secret.created".into(),
            payload: serde_json::json!({"scope": "repo", "name": name, "repo": repo}),
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(meta)))
}

async fn get_repo_secret(
    State(state): State<Arc<AppState>>,
    Path((repo, name)): Path<(String, String)>,
) -> Result<Json<SecretMeta>, ApiError> {
    let pool = require_pool(&state)?;
    let repo_row = control::get_repo_by_name(pool, &repo).await?;
    let meta = get_by_repo_name(pool, &repo_row.id, &name).await?;
    Ok(Json(meta))
}

async fn patch_repo_secret(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((repo, name)): Path<(String, String)>,
    Json(body): Json<PatchSecretRequest>,
) -> Result<Json<SecretMeta>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = require_pool(&state)?;
    let crypto = require_crypto(&state)?;
    let clotho = control::require_repo_admin(pool, &repo, &auth.user_id).await?;
    let existing = get_by_repo_name(pool, &clotho.repo.id, &name).await?;
    let description = body
        .description
        .as_deref()
        .map(str::trim)
        .unwrap_or(existing.description.as_str());
    let value = match &body.value {
        Some(v) if !v.is_empty() => v.as_str(),
        Some(_) => {
            return Err(ApiError::InvalidRequest(
                "secret value cannot be empty; omit value to keep current".into(),
            ))
        }
        None => "",
    };
    let meta = if value.is_empty() {
        update_description(pool, &existing.id, description).await?
    } else {
        upsert_repo_secret(
            pool,
            crypto,
            &clotho.repo.org_id,
            &clotho.repo.id,
            &name,
            value,
            description,
            &auth.user_id,
            false,
        )
        .await?
    };
    control::log_activity(
        pool,
        ActivityEventInput {
            actor_id: auth.user_id.clone(),
            org_id: Some(clotho.repo.org_id),
            repo_id: Some(clotho.repo.id),
            event_type: "secret.updated".into(),
            payload: serde_json::json!({"scope": "repo", "name": name, "repo": repo}),
        },
    )
    .await?;
    Ok(Json(meta))
}

async fn delete_repo_secret(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((repo, name)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = require_pool(&state)?;
    let clotho = control::require_repo_admin(pool, &repo, &auth.user_id).await?;
    let deleted = delete_by_repo_name(pool, &clotho.repo.id, &name).await?;
    if !deleted {
        return Err(ApiError::NotFound(format!("secret {name:?} not found")));
    }
    control::log_activity(
        pool,
        ActivityEventInput {
            actor_id: auth.user_id.clone(),
            org_id: Some(clotho.repo.org_id),
            repo_id: Some(clotho.repo.id),
            event_type: "secret.deleted".into(),
            payload: serde_json::json!({"scope": "repo", "name": name, "repo": repo}),
        },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// provider connect / disconnect convenience
// ---------------------------------------------------------------------------

async fn connect_provider(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(provider): Path<String>,
    Json(body): Json<ConnectProviderRequest>,
) -> Result<Json<SecretMeta>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = require_pool(&state)?;
    let crypto = require_crypto(&state)?;
    let provider_id = provider.to_lowercase();
    let org_name = if body.org.trim().is_empty() {
        state.bootstrap.org_name.clone()
    } else {
        body.org.trim().to_string()
    };
    let org_row = control::get_org(pool, &org_name).await?;
    control::require_org_role(pool, &org_row.id, &auth.user_id, "admin").await?;

    // ComputeSDK: store credentials for any catalogued upstream.
    if provider_id == "computesdk" {
        let meta =
            connect_computesdk_upstream(pool, crypto, &org_row.id, &auth.user_id, &body).await?;
        control::log_activity(
            pool,
            ActivityEventInput {
                actor_id: auth.user_id.clone(),
                org_id: Some(org_row.id),
                repo_id: None,
                event_type: "provider.connected".into(),
                payload: serde_json::json!({
                    "provider": provider_id,
                    "upstream": body.upstream.trim().to_lowercase(),
                    "secret_name": meta.name,
                }),
            },
        )
        .await?;
        return Ok(Json(meta));
    }

    if body.api_key.trim().is_empty() {
        return Err(ApiError::InvalidRequest("api_key is required".into()));
    }
    let (secret_name, description) = match provider_id.as_str() {
        "daytona" => (
            SECRET_DAYTONA_API_KEY,
            "Daytona API key for Actions and sandboxes",
        ),
        "box" => (SECRET_BOX_API_KEY, "Box API key for Actions and sandboxes"),
        other => {
            return Err(ApiError::InvalidRequest(format!(
                "provider {other:?} does not support in-app connect"
            )))
        }
    };
    let meta = upsert_org_secret(
        pool,
        crypto,
        &org_row.id,
        secret_name,
        body.api_key.trim(),
        description,
        &auth.user_id,
        false,
    )
    .await?;
    control::log_activity(
        pool,
        ActivityEventInput {
            actor_id: auth.user_id.clone(),
            org_id: Some(org_row.id),
            repo_id: None,
            event_type: "provider.connected".into(),
            payload: serde_json::json!({
                "provider": provider_id,
                "secret_name": secret_name,
            }),
        },
    )
    .await?;
    Ok(Json(meta))
}

/// Remove Clotho-stored credentials for a provider (metadata only; no values).
async fn disconnect_provider(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(provider): Path<String>,
    Query(query): Query<DisconnectQuery>,
) -> Result<Json<DisconnectProviderResponse>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = require_pool(&state)?;
    let provider_id = provider.to_lowercase();
    let names = provider_secret_names(&provider_id);
    if names.is_empty() {
        return Err(ApiError::InvalidRequest(format!(
            "provider {provider_id:?} has no disconnectable credentials"
        )));
    }
    let org_name = if query.org.trim().is_empty() {
        state.bootstrap.org_name.clone()
    } else {
        query.org.trim().to_string()
    };
    let org_row = control::get_org(pool, &org_name).await?;
    control::require_org_role(pool, &org_row.id, &auth.user_id, "admin").await?;
    let mut deleted_secrets = Vec::new();
    for name in names {
        if delete_by_org_name(pool, &org_row.id, name).await? {
            deleted_secrets.push(name.to_string());
        }
    }
    if deleted_secrets.is_empty() {
        return Err(ApiError::NotFound(format!(
            "no Clotho secrets stored for provider {provider_id}"
        )));
    }
    control::log_activity(
        pool,
        ActivityEventInput {
            actor_id: auth.user_id.clone(),
            org_id: Some(org_row.id),
            repo_id: None,
            event_type: "provider.disconnected".into(),
            payload: serde_json::json!({
                "provider": provider_id,
                "deleted_secrets": deleted_secrets,
            }),
        },
    )
    .await?;
    Ok(Json(DisconnectProviderResponse {
        provider: provider_id,
        deleted_secrets,
    }))
}

#[derive(Deserialize)]
struct DisconnectQuery {
    #[serde(default)]
    org: String,
}

// ---------------------------------------------------------------------------
// resolution helpers (used by Actions / provider overlay)
// ---------------------------------------------------------------------------

/// Map provider id → primary well-known secret name (single-key providers).
pub fn provider_secret_name(provider_id: &str) -> Option<&'static str> {
    match provider_id.to_lowercase().as_str() {
        "daytona" => Some(SECRET_DAYTONA_API_KEY),
        "box" => Some(SECRET_BOX_API_KEY),
        // Any connected ComputeSDK secret indicates overlay readiness.
        "computesdk" => computesdk_catalog::all_secret_names().into_iter().next(),
        _ => None,
    }
}

/// All secret names that bind to a provider (for disconnect / multi-key).
pub fn provider_secret_names(provider_id: &str) -> Vec<&'static str> {
    match provider_id.to_lowercase().as_str() {
        "daytona" => vec![SECRET_DAYTONA_API_KEY],
        "box" => vec![SECRET_BOX_API_KEY],
        "computesdk" => computesdk_catalog::all_secret_names(),
        _ => vec![],
    }
}

/// Resolve plaintext provider API key for a repo: repo secret overrides org.
/// Returns None when no secret is stored (env may still apply on compute).
pub async fn resolve_provider_api_key(
    state: &AppState,
    repo_name: &str,
    provider_id: &str,
) -> Result<Option<String>, ApiError> {
    let Some(secret_name) = provider_secret_name(provider_id) else {
        return Ok(None);
    };
    // For computesdk, prefer the first present secret rather than only the
    // alphabetically first catalog name.
    if provider_id.eq_ignore_ascii_case("computesdk") {
        let creds = resolve_computesdk_credentials(state, repo_name).await?;
        return Ok(creds.into_values().next());
    }
    resolve_named_secret(state, repo_name, secret_name).await
}

/// Resolve all ComputeSDK upstream credentials for injection into CCI jobs.
/// Keys are UPPER_SNAKE env names the bridge catalog understands.
pub async fn resolve_computesdk_credentials(
    state: &AppState,
    repo_name: &str,
) -> Result<std::collections::HashMap<String, String>, ApiError> {
    let mut out = std::collections::HashMap::new();
    for name in computesdk_catalog::all_secret_names() {
        if let Some(v) = resolve_named_secret(state, repo_name, name).await? {
            out.insert(name.to_string(), v);
        }
    }
    Ok(out)
}

/// Store credentials for one ComputeSDK upstream (any catalogued provider).
async fn connect_computesdk_upstream(
    pool: &PgPool,
    crypto: &SecretsCrypto,
    org_id: &str,
    user_id: &str,
    body: &ConnectProviderRequest,
) -> Result<SecretMeta, ApiError> {
    let mut creds = body.credentials.clone();
    // Back-compat: modal_token_* fields and bare api_key.
    if !body.modal_token_id.trim().is_empty() {
        creds.insert(
            "MODAL_TOKEN_ID".into(),
            body.modal_token_id.trim().to_string(),
        );
    }
    if !body.modal_token_secret.trim().is_empty() {
        creds.insert(
            "MODAL_TOKEN_SECRET".into(),
            body.modal_token_secret.trim().to_string(),
        );
    }

    let upstream_id = {
        let u = body.upstream.trim().to_lowercase();
        if u.is_empty() {
            // Infer from credential keys or default e2b when only api_key set.
            if creds.contains_key("MODAL_TOKEN_ID") || creds.contains_key("modal_token_id") {
                "modal".into()
            } else if creds
                .keys()
                .any(|k| k.to_uppercase().starts_with("VERCEL_"))
            {
                "vercel".into()
            } else if !body.api_key.trim().is_empty() && creds.is_empty() {
                "e2b".into()
            } else if let Some(first) = creds.keys().next() {
                // Match any upstream whose required list is subset of keys.
                infer_upstream_from_keys(&creds).unwrap_or_else(|| first.clone())
            } else {
                "e2b".into()
            }
        } else {
            u
        }
    };

    let Some(spec) = computesdk_catalog::find_upstream(&upstream_id) else {
        let known: Vec<_> = computesdk_catalog::UPSTREAMS.iter().map(|u| u.id).collect();
        return Err(ApiError::InvalidRequest(format!(
            "unknown ComputeSDK upstream {upstream_id:?}; known: {known:?}"
        )));
    };

    // Single-key convenience: api_key → first required secret.
    if !body.api_key.trim().is_empty() && spec.required.len() == 1 {
        creds
            .entry(spec.required[0].to_string())
            .or_insert_with(|| body.api_key.trim().to_string());
    }

    // Normalize keys to UPPER_SNAKE.
    let mut normalized = std::collections::HashMap::new();
    for (k, v) in creds {
        if v.trim().is_empty() {
            continue;
        }
        let key = k.trim().to_uppercase().replace('-', "_");
        valid_secret_name(&key)?;
        normalized.insert(key, v.trim().to_string());
    }

    // Require all required fields for this upstream (k8s may have none).
    for req in spec.required {
        if !normalized.contains_key(*req) {
            return Err(ApiError::InvalidRequest(format!(
                "ComputeSDK upstream {:?} requires secret {req} (pass credentials.{req} or api_key for single-key providers)",
                spec.id
            )));
        }
    }
    if normalized.is_empty() && spec.required.is_empty() {
        // k8s with optional-only: require at least one optional secret.
        return Err(ApiError::InvalidRequest(format!(
            "ComputeSDK upstream {:?} needs at least one of {:?}",
            spec.id, spec.optional
        )));
    }
    if normalized.is_empty() {
        return Err(ApiError::InvalidRequest(
            "at least one credential value is required".into(),
        ));
    }

    let mut last_meta = None;
    for (name, value) in &normalized {
        let desc = format!("ComputeSDK {} ({})", spec.name, name);
        let meta =
            upsert_org_secret(pool, crypto, org_id, name, value, &desc, user_id, false).await?;
        last_meta = Some(meta);
    }
    last_meta.ok_or_else(|| ApiError::Internal("no secrets written".into()))
}

fn infer_upstream_from_keys(creds: &std::collections::HashMap<String, String>) -> Option<String> {
    let keys: std::collections::HashSet<String> = creds
        .keys()
        .map(|k| k.to_uppercase().replace('-', "_"))
        .collect();
    for u in computesdk_catalog::UPSTREAMS {
        if u.required.is_empty() {
            continue;
        }
        if u.required.iter().all(|r| keys.contains(*r)) {
            return Some(u.id.to_string());
        }
    }
    None
}

async fn resolve_named_secret(
    state: &AppState,
    repo_name: &str,
    secret_name: &str,
) -> Result<Option<String>, ApiError> {
    let Some(pool) = state.pool.as_ref() else {
        return Ok(None);
    };
    let Some(crypto) = state.secrets_crypto.as_ref() else {
        return Ok(None);
    };

    // Prefer repo-scoped secret when the repo exists in the control plane.
    if let Ok(repo_row) = control::get_repo_by_name(pool, repo_name).await {
        if let Ok(Some(ct)) = load_ciphertext_repo(pool, &repo_row.id, secret_name).await {
            let plain = crypto.open(&ct)?;
            return Ok(Some(
                String::from_utf8(plain)
                    .map_err(|e| ApiError::Internal(format!("secret utf-8: {e}")))?,
            ));
        }
        if let Ok(Some(ct)) = load_ciphertext_org(pool, &repo_row.org_id, secret_name).await {
            let plain = crypto.open(&ct)?;
            return Ok(Some(
                String::from_utf8(plain)
                    .map_err(|e| ApiError::Internal(format!("secret utf-8: {e}")))?,
            ));
        }
        return Ok(None);
    }

    // Fall back to bootstrap org when repo isn't in control plane.
    if let Ok(Some(ct)) = load_ciphertext_org(pool, &state.bootstrap.org_id, secret_name).await {
        let plain = crypto.open(&ct)?;
        return Ok(Some(
            String::from_utf8(plain)
                .map_err(|e| ApiError::Internal(format!("secret utf-8: {e}")))?,
        ));
    }
    Ok(None)
}

/// Whether a provider has a Clotho-stored credential (settings overlay).
pub async fn provider_secret_configured(state: &AppState, provider_id: &str) -> Option<SecretMeta> {
    let pool = state.pool.as_ref()?;
    let names = provider_secret_names(provider_id);
    if names.is_empty() {
        return None;
    }
    // Prefer bootstrap org; return first matching secret for mask display.
    for name in names {
        if let Ok(meta) = get_by_org_name(pool, &state.bootstrap.org_id, name).await {
            return Some(meta);
        }
    }
    None
}

/// Whether ComputeSDK has enough Clotho secrets to accept a job (with bridge up).
/// True when any catalogued upstream has its required secrets present.
pub async fn computesdk_secrets_ready(state: &AppState) -> bool {
    let pool = match state.pool.as_ref() {
        Some(p) => p,
        None => return false,
    };
    let org = state.bootstrap.org_id.as_str();
    for upstream in computesdk_catalog::UPSTREAMS {
        if upstream.required.is_empty() {
            // Optional-only (e.g. k8s): any optional secret counts.
            for name in upstream.optional {
                if get_by_org_name(pool, org, name).await.is_ok() {
                    return true;
                }
            }
            continue;
        }
        let mut all = true;
        for name in upstream.required {
            if get_by_org_name(pool, org, name).await.is_err() {
                all = false;
                break;
            }
        }
        if all {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// SQL
// ---------------------------------------------------------------------------

async fn list_by_org(pool: &PgPool, org_id: &str) -> Result<Vec<SecretMeta>, ApiError> {
    sqlx::query_as::<_, SecretMeta>(
        r#"
        select id, scope, org_id, repo_id, name, description, value_last4,
               created_by, created_at, updated_at
        from secrets
        where scope = 'org' and org_id = $1
        order by name
        "#,
    )
    .bind(org_id)
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("list org secrets: {e}")))
}

async fn list_by_repo(pool: &PgPool, repo_id: &str) -> Result<Vec<SecretMeta>, ApiError> {
    sqlx::query_as::<_, SecretMeta>(
        r#"
        select id, scope, org_id, repo_id, name, description, value_last4,
               created_by, created_at, updated_at
        from secrets
        where scope = 'repo' and repo_id = $1
        order by name
        "#,
    )
    .bind(repo_id)
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("list repo secrets: {e}")))
}

async fn get_by_org_name(pool: &PgPool, org_id: &str, name: &str) -> Result<SecretMeta, ApiError> {
    sqlx::query_as::<_, SecretMeta>(
        r#"
        select id, scope, org_id, repo_id, name, description, value_last4,
               created_by, created_at, updated_at
        from secrets
        where scope = 'org' and org_id = $1 and name = $2
        "#,
    )
    .bind(org_id)
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("get org secret: {e}")))?
    .ok_or_else(|| ApiError::NotFound(format!("secret {name:?} not found")))
}

async fn get_by_repo_name(
    pool: &PgPool,
    repo_id: &str,
    name: &str,
) -> Result<SecretMeta, ApiError> {
    sqlx::query_as::<_, SecretMeta>(
        r#"
        select id, scope, org_id, repo_id, name, description, value_last4,
               created_by, created_at, updated_at
        from secrets
        where scope = 'repo' and repo_id = $1 and name = $2
        "#,
    )
    .bind(repo_id)
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("get repo secret: {e}")))?
    .ok_or_else(|| ApiError::NotFound(format!("secret {name:?} not found")))
}

#[allow(clippy::too_many_arguments)]
async fn upsert_org_secret(
    pool: &PgPool,
    crypto: &SecretsCrypto,
    org_id: &str,
    name: &str,
    value: &str,
    description: &str,
    created_by: &str,
    create_only: bool,
) -> Result<SecretMeta, ApiError> {
    let ciphertext = crypto.seal(value.as_bytes())?;
    let mask = last4(value);
    let id = Uuid::new_v4().to_string();

    if create_only {
        let existing = sqlx::query_scalar::<_, i64>(
            "select count(*) from secrets where scope = 'org' and org_id = $1 and name = $2",
        )
        .bind(org_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("check secret: {e}")))?;
        if existing > 0 {
            return Err(ApiError::Conflict(format!(
                "secret {name:?} already exists; use rotate instead"
            )));
        }
    }

    // Delete + insert so we do not depend on partial unique index ON CONFLICT.
    sqlx::query("delete from secrets where scope = 'org' and org_id = $1 and name = $2")
        .bind(org_id)
        .bind(name)
        .execute(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("rotate org secret: {e}")))?;

    sqlx::query(
        r#"
        insert into secrets (id, scope, org_id, repo_id, name, description, ciphertext, value_last4, created_by)
        values ($1, 'org', $2, null, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(&id)
    .bind(org_id)
    .bind(name)
    .bind(description)
    .bind(&ciphertext)
    .bind(&mask)
    .bind(created_by)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("insert org secret: {e}")))?;

    get_by_org_name(pool, org_id, name).await
}

#[allow(clippy::too_many_arguments)]
async fn upsert_repo_secret(
    pool: &PgPool,
    crypto: &SecretsCrypto,
    org_id: &str,
    repo_id: &str,
    name: &str,
    value: &str,
    description: &str,
    created_by: &str,
    create_only: bool,
) -> Result<SecretMeta, ApiError> {
    let ciphertext = crypto.seal(value.as_bytes())?;
    let mask = last4(value);
    let id = Uuid::new_v4().to_string();

    if create_only {
        let existing = sqlx::query_scalar::<_, i64>(
            "select count(*) from secrets where scope = 'repo' and repo_id = $1 and name = $2",
        )
        .bind(repo_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("check secret: {e}")))?;
        if existing > 0 {
            return Err(ApiError::Conflict(format!(
                "secret {name:?} already exists; use rotate instead"
            )));
        }
    }

    // Delete + insert for repo upsert to avoid partial unique index issues.
    sqlx::query("delete from secrets where scope = 'repo' and repo_id = $1 and name = $2")
        .bind(repo_id)
        .bind(name)
        .execute(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("rotate repo secret: {e}")))?;

    sqlx::query(
        r#"
        insert into secrets (id, scope, org_id, repo_id, name, description, ciphertext, value_last4, created_by)
        values ($1, 'repo', $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(&id)
    .bind(org_id)
    .bind(repo_id)
    .bind(name)
    .bind(description)
    .bind(&ciphertext)
    .bind(&mask)
    .bind(created_by)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("insert repo secret: {e}")))?;

    get_by_repo_name(pool, repo_id, name).await
}

async fn update_description(
    pool: &PgPool,
    id: &str,
    description: &str,
) -> Result<SecretMeta, ApiError> {
    sqlx::query(
        r#"
        update secrets set description = $2, updated_at = now()
        where id = $1
        "#,
    )
    .bind(id)
    .bind(description)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("update secret description: {e}")))?;

    sqlx::query_as::<_, SecretMeta>(
        r#"
        select id, scope, org_id, repo_id, name, description, value_last4,
               created_by, created_at, updated_at
        from secrets where id = $1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("reload secret: {e}")))
}

async fn delete_by_org_name(pool: &PgPool, org_id: &str, name: &str) -> Result<bool, ApiError> {
    let res = sqlx::query("delete from secrets where scope = 'org' and org_id = $1 and name = $2")
        .bind(org_id)
        .bind(name)
        .execute(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("delete org secret: {e}")))?;
    Ok(res.rows_affected() > 0)
}

async fn delete_by_repo_name(pool: &PgPool, repo_id: &str, name: &str) -> Result<bool, ApiError> {
    let res =
        sqlx::query("delete from secrets where scope = 'repo' and repo_id = $1 and name = $2")
            .bind(repo_id)
            .bind(name)
            .execute(pool)
            .await
            .map_err(|e| ApiError::Internal(format!("delete repo secret: {e}")))?;
    Ok(res.rows_affected() > 0)
}

async fn load_ciphertext_org(
    pool: &PgPool,
    org_id: &str,
    name: &str,
) -> Result<Option<Vec<u8>>, ApiError> {
    sqlx::query_scalar::<_, Vec<u8>>(
        "select ciphertext from secrets where scope = 'org' and org_id = $1 and name = $2",
    )
    .bind(org_id)
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("load org ciphertext: {e}")))
}

async fn load_ciphertext_repo(
    pool: &PgPool,
    repo_id: &str,
    name: &str,
) -> Result<Option<Vec<u8>>, ApiError> {
    sqlx::query_scalar::<_, Vec<u8>>(
        "select ciphertext from secrets where scope = 'repo' and repo_id = $1 and name = $2",
    )
    .bind(repo_id)
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("load repo ciphertext: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_roundtrip() {
        let key = [7u8; 32];
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let crypto = SecretsCrypto { cipher };
        let sealed = crypto.seal(b"super-secret-key").unwrap();
        assert_ne!(sealed, b"super-secret-key");
        let opened = crypto.open(&sealed).unwrap();
        assert_eq!(opened, b"super-secret-key");
    }

    #[test]
    fn decode_hex_and_b64() {
        let hex = "00".repeat(32);
        assert_eq!(decode_master_key(&hex).unwrap(), [0u8; 32]);
        let b64 = base64::engine::general_purpose::STANDARD.encode([1u8; 32]);
        assert_eq!(decode_master_key(&b64).unwrap(), [1u8; 32]);
    }

    #[test]
    fn last4_mask() {
        assert_eq!(last4("abcdefgh"), "efgh");
        assert_eq!(last4("ab"), "ab");
        assert_eq!(last4(""), "");
    }
}
