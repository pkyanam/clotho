//! Common persisted idempotency-key helpers for retryable REST mutations.

use axum::http::HeaderMap;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

use crate::error::ApiError;

pub const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";
pub const IDEMPOTENCY_REPLAYED_HEADER: &str = "idempotency-replayed";
pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 128;
pub const MAX_STORED_RESPONSE_BYTES: usize = 64 * 1024;
const CLEANUP_BATCH_SIZE: i64 = 1_000;

pub struct StoredResponse {
    pub operation: String,
    pub request_fingerprint: String,
    pub resource_id: String,
    pub response_status: i32,
    pub response_body: serde_json::Value,
}

pub fn extract_key(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    let Some(value) = headers.get(IDEMPOTENCY_KEY_HEADER) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| ApiError::InvalidRequest("Idempotency-Key must be ASCII".into()))?;
    if value.is_empty()
        || value.len() > MAX_IDEMPOTENCY_KEY_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(ApiError::InvalidRequest(format!(
            "Idempotency-Key must be 1..{MAX_IDEMPOTENCY_KEY_BYTES} characters using letters, digits, '.', '_', ':', or '-'"
        )));
    }
    Ok(Some(value.to_owned()))
}

pub fn key_hash(key: &str) -> String {
    sha256(key.as_bytes())
}

pub fn fingerprint<T: Serialize>(value: &T) -> Result<String, ApiError> {
    let canonical = serde_json::to_vec(value)
        .map_err(|error| ApiError::Internal(format!("serialize idempotent request: {error}")))?;
    Ok(sha256(&canonical))
}

pub async fn lookup(
    pool: &PgPool,
    org_id: &str,
    principal_id: &str,
    key: &str,
) -> Result<Option<StoredResponse>, ApiError> {
    let row = sqlx::query(
        r#"select operation, request_fingerprint, resource_id,
                  response_status, response_body
           from idempotency_records
           where org_id = $1 and principal_id = $2 and key_hash = $3
             and expires_at > now()"#,
    )
    .bind(org_id)
    .bind(principal_id)
    .bind(key_hash(key))
    .fetch_optional(pool)
    .await
    .map_err(|error| ApiError::Internal(format!("lookup idempotency key: {error}")))?;
    Ok(row.map(|row| StoredResponse {
        operation: row.get("operation"),
        request_fingerprint: row.get("request_fingerprint"),
        resource_id: row.get("resource_id"),
        response_status: row.get("response_status"),
        response_body: row.get("response_body"),
    }))
}

pub fn require_match(
    stored: &StoredResponse,
    operation: &str,
    request_fingerprint: &str,
) -> Result<(), ApiError> {
    if stored.operation != operation || stored.request_fingerprint != request_fingerprint {
        return Err(ApiError::IdempotencyConflict(
            "Idempotency-Key was already used for a different request".into(),
        ));
    }
    Ok(())
}

pub fn start_cleanup(pool: Option<PgPool>) {
    let Some(pool) = pool else { return };
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    runtime.spawn(async move {
        loop {
            if let Err(error) = cleanup_expired(&pool).await {
                tracing::warn!(%error, "failed to clean expired idempotency records");
            }
            tokio::time::sleep(std::time::Duration::from_secs(60 * 60)).await;
        }
    });
}

async fn cleanup_expired(pool: &PgPool) -> Result<u64, ApiError> {
    sqlx::query(
        r#"delete from idempotency_records
           where ctid in (
             select ctid from idempotency_records
             where expires_at <= now()
             order by expires_at
             limit $1
           )"#,
    )
    .bind(CLEANUP_BATCH_SIZE)
    .execute(pool)
    .await
    .map(|result| result.rows_affected())
    .map_err(|error| ApiError::Internal(format!("clean expired idempotency records: {error}")))
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_bounded_and_fingerprints_are_deterministic() {
        let mut headers = HeaderMap::new();
        headers.insert(IDEMPOTENCY_KEY_HEADER, "retry_01:action".parse().unwrap());
        assert_eq!(
            extract_key(&headers).unwrap().as_deref(),
            Some("retry_01:action")
        );

        headers.insert(IDEMPOTENCY_KEY_HEADER, "contains space".parse().unwrap());
        assert!(extract_key(&headers).is_err());
        headers.insert(
            IDEMPOTENCY_KEY_HEADER,
            "a".repeat(MAX_IDEMPOTENCY_KEY_BYTES + 1).parse().unwrap(),
        );
        assert!(extract_key(&headers).is_err());

        let request = serde_json::json!({"repo_id": "repo-1", "workflow": "ci"});
        assert_eq!(
            fingerprint(&request).unwrap(),
            fingerprint(&request).unwrap()
        );
        assert_ne!(key_hash("key-a"), key_hash("key-b"));
    }
}
