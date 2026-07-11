//! Forgejo push-webhook receiver → Stage 7 CI (docs/adr/0008).
//!
//! Forgejo POSTs here on every push (the hook is registered per repo at
//! creation time). Admission fails closed unless the exact request bytes have
//! a valid HMAC, a bounded provider delivery id, a unique Clotho repository,
//! and a durable replay reservation. Only the transaction winner schedules
//! CI; exact retries are harmless and changed payloads under one id conflict.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

use crate::error::ApiError;
use crate::{ci, control, AppState};

const MAX_DELIVERY_ID_BYTES: usize = 128;
const CLEANUP_BATCH_SIZE: i64 = 1_000;
const DELIVERY_RETENTION_HOURS: i64 = 24;

#[derive(Deserialize)]
struct PushPayload {
    /// Tip commit after the push; all-zero for a branch deletion.
    #[serde(default)]
    after: String,
    #[serde(default)]
    repository: Repository,
}

#[derive(Deserialize, Default)]
struct Repository {
    #[serde(default)]
    name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reservation {
    New,
    Replay,
}

/// Handle one internal collaboration-provider delivery. CI stays detached,
/// but only after the delivery reservation commits durably.
pub async fn forgejo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_signature(&state.webhook_secret, &headers, &body)?;
    let delivery_hash = sha256(delivery_id(&headers)?.as_bytes());
    let payload_hash = sha256(&body);
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("webhook replay database is not configured".into()))?;

    // Require signed, identified deliveries even for ignored provider events.
    // Non-push events have no Clotho side effect and therefore need no replay
    // row; actionable push deliveries are reserved below.
    let event = event_name(&headers)?;
    if event != "push" {
        return Ok(ignored(format!("event {event:?} ignored")));
    }

    let payload: PushPayload = serde_json::from_slice(&body)
        .map_err(|_| ApiError::InvalidRequest("invalid webhook payload".into()))?;
    let repo = payload.repository.name;
    let sha = payload.after;
    if repo.is_empty() {
        return Err(ApiError::InvalidRequest(
            "webhook payload is missing repository name".into(),
        ));
    }
    if !valid_commit_sha(&sha) {
        return Err(ApiError::InvalidRequest(
            "webhook payload has an invalid commit sha".into(),
        ));
    }

    // Name-routed webhooks are safe only while exactly one Clotho repository
    // owns the provider name. Missing and ambiguous names fail before any CI
    // or Forgejo side effect.
    let clotho = control::get_repo_with_org(pool, &repo)
        .await?
        .ok_or_else(|| ApiError::NotFound("webhook repository not found".into()))?;

    let reservation = reserve_delivery(
        pool,
        &delivery_hash,
        &payload_hash,
        &clotho.repo.org_id,
        &clotho.repo.id,
        "push",
        &sha,
    )
    .await?;

    // Opportunistic retention keeps work bounded per admitted delivery. A
    // cleanup failure does not invalidate the reservation, but it is visible
    // without logging the provider id, body, signature, or shared secret.
    if let Err(error) = cleanup_expired(pool).await {
        tracing::warn!(%error, "failed to clean expired webhook deliveries");
    }

    if reservation == Reservation::Replay {
        tracing::info!(%repo, %sha, "push webhook replay acknowledged");
        return Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "replayed",
                "repo": repo,
                "sha": sha,
            })),
        ));
    }

    // Branch deletion (all-zero sha) has nothing to check, but remains
    // reserved so retries cannot change intent under the provider id.
    if sha.bytes().all(|byte| byte == b'0') {
        return Ok(ignored("branch deletion, nothing to check".to_string()));
    }

    tracing::info!(%repo, %sha, "push webhook accepted; starting ci");
    tokio::spawn(ci::run(state, repo.clone(), sha.clone()));
    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "status": "accepted", "repo": repo, "sha": sha })),
    ))
}

fn ignored(reason: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "ignored", "reason": reason })),
    )
}

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn event_name(headers: &HeaderMap) -> Result<String, ApiError> {
    let forgejo = header(headers, "x-forgejo-event");
    let gitea = header(headers, "x-gitea-event");
    let event = match (forgejo, gitea) {
        (Some(forgejo), Some(gitea)) if !forgejo.eq_ignore_ascii_case(&gitea) => {
            return Err(ApiError::InvalidRequest(
                "conflicting webhook event headers".into(),
            ));
        }
        (Some(value), _) | (_, Some(value)) => value,
        (None, None) => {
            return Err(ApiError::InvalidRequest(
                "missing X-Forgejo-Event or X-Gitea-Event header".into(),
            ));
        }
    };
    if event.is_empty()
        || event.len() > 64
        || !event
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ApiError::InvalidRequest(
            "invalid webhook event header".into(),
        ));
    }
    Ok(event.to_ascii_lowercase())
}

fn valid_commit_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn delivery_id(headers: &HeaderMap) -> Result<String, ApiError> {
    let forgejo = header(headers, "x-forgejo-delivery");
    let gitea = header(headers, "x-gitea-delivery");
    let value = match (forgejo, gitea) {
        (Some(forgejo), Some(gitea)) if forgejo != gitea => {
            return Err(ApiError::InvalidRequest(
                "conflicting webhook delivery headers".into(),
            ));
        }
        (Some(value), _) | (_, Some(value)) => value,
        (None, None) => {
            return Err(ApiError::InvalidRequest(
                "missing X-Forgejo-Delivery or X-Gitea-Delivery header".into(),
            ));
        }
    };
    if value.is_empty()
        || value.len() > MAX_DELIVERY_ID_BYTES
        || !value.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(ApiError::InvalidRequest(format!(
            "webhook delivery id must be 1..{MAX_DELIVERY_ID_BYTES} visible ASCII characters"
        )));
    }
    Ok(value)
}

fn require_signature(secret: &str, headers: &HeaderMap, body: &[u8]) -> Result<(), ApiError> {
    if secret.is_empty() {
        return Err(ApiError::Internal(
            "webhook signing secret is not configured".into(),
        ));
    }
    let signature = header(headers, "x-forgejo-signature")
        .or_else(|| header(headers, "x-gitea-signature"))
        .or_else(|| {
            header(headers, "x-hub-signature-256")
                .and_then(|value| value.strip_prefix("sha256=").map(str::to_owned))
        })
        .ok_or_else(|| ApiError::Unauthorized("invalid webhook signature".into()))?;
    if !verify(secret, body, &signature) {
        return Err(ApiError::Unauthorized("invalid webhook signature".into()));
    }
    Ok(())
}

/// Constant-time HMAC-SHA256 check of the raw body against a hex signature.
fn verify(secret: &str, body: &[u8], sig_hex: &str) -> bool {
    if sig_hex.len() != 64 {
        return false;
    }
    let Ok(expected) = decode_hex(sig_hex) else {
        return false;
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac accepts any key");
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

fn decode_hex(value: &str) -> Result<Vec<u8>, ()> {
    if !value.len().is_multiple_of(2) {
        return Err(());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| ()))
        .collect()
}

fn sha256(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    format!("{:x}", hasher.finalize())
}

#[allow(clippy::too_many_arguments)]
async fn reserve_delivery(
    pool: &PgPool,
    delivery_hash: &str,
    payload_hash: &str,
    org_id: &str,
    repo_id: &str,
    event_type: &str,
    commit_sha: &str,
) -> Result<Reservation, ApiError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|error| ApiError::Internal(format!("begin webhook reservation: {error}")))?;

    // Permit reuse only after the documented retention window even if the
    // opportunistic cleanup has not reached this row yet.
    sqlx::query("delete from webhook_deliveries where delivery_hash = $1 and expires_at <= now()")
        .bind(delivery_hash)
        .execute(&mut *transaction)
        .await
        .map_err(|error| ApiError::Internal(format!("expire webhook reservation: {error}")))?;

    let inserted = sqlx::query(
        r#"insert into webhook_deliveries
           (delivery_hash, payload_hash, org_id, repo_id, event_type, commit_sha, expires_at)
           values ($1, $2, $3, $4, $5, $6, now() + make_interval(hours => $7))
           on conflict (delivery_hash) do nothing"#,
    )
    .bind(delivery_hash)
    .bind(payload_hash)
    .bind(org_id)
    .bind(repo_id)
    .bind(event_type)
    .bind(commit_sha)
    .bind(DELIVERY_RETENTION_HOURS as i32)
    .execute(&mut *transaction)
    .await
    .map_err(|error| ApiError::Internal(format!("reserve webhook delivery: {error}")))?
    .rows_affected()
        == 1;

    let reservation = if inserted {
        Reservation::New
    } else {
        let row = sqlx::query(
            r#"select payload_hash, org_id, repo_id, event_type, commit_sha
               from webhook_deliveries
               where delivery_hash = $1
               for update"#,
        )
        .bind(delivery_hash)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| ApiError::Internal(format!("read webhook reservation: {error}")))?
        .ok_or_else(|| ApiError::Internal("webhook reservation disappeared".into()))?;
        let matches = row.get::<String, _>("payload_hash") == payload_hash
            && row.get::<String, _>("org_id") == org_id
            && row.get::<String, _>("repo_id") == repo_id
            && row.get::<String, _>("event_type") == event_type
            && row.get::<String, _>("commit_sha") == commit_sha;
        if !matches {
            return Err(ApiError::Conflict(
                "webhook delivery id was already used for a different payload".into(),
            ));
        }
        Reservation::Replay
    };

    transaction
        .commit()
        .await
        .map_err(|error| ApiError::Internal(format!("commit webhook reservation: {error}")))?;
    Ok(reservation)
}

async fn cleanup_expired(pool: &PgPool) -> Result<u64, ApiError> {
    sqlx::query(
        r#"delete from webhook_deliveries
           where ctid in (
             select ctid from webhook_deliveries
             where expires_at <= now()
             order by expires_at
             limit $1
           )"#,
    )
    .bind(CLEANUP_BATCH_SIZE)
    .execute(pool)
    .await
    .map(|result| result.rows_affected())
    .map_err(|error| ApiError::Internal(format!("clean expired webhook deliveries: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_headers(secret: &str, body: &[u8], delivery_id: &str) -> HeaderMap {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let signature = mac.finalize().into_bytes();
        let encoded = signature
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let mut headers = HeaderMap::new();
        headers.insert("x-forgejo-signature", encoded.parse().unwrap());
        headers.insert("x-forgejo-delivery", delivery_id.parse().unwrap());
        headers.insert("x-forgejo-event", "push".parse().unwrap());
        headers
    }

    fn database_url() -> Option<String> {
        for name in [
            "CLOTHO_WEBHOOK_TEST_DATABASE_URL",
            "CLOTHO_STAGE11_TEST_DATABASE_URL",
            "CLOTHO_CONTROL_PLANE_TEST_DATABASE_URL",
        ] {
            if let Ok(value) = std::env::var(name) {
                if !value.trim().is_empty() {
                    return Some(value);
                }
            }
        }
        if std::env::var("CLOTHO_TEST_FAIL_ON_SKIP")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false)
        {
            panic!("webhook replay test requires CLOTHO_WEBHOOK_TEST_DATABASE_URL");
        }
        eprintln!("skipping webhook replay DB test: no database URL configured");
        None
    }

    #[test]
    fn signing_and_delivery_headers_fail_closed_and_stay_bounded() {
        let secret = "test-only-signing-key";
        let body = br#"{"after":"abc","repository":{"name":"repo"}}"#;
        let headers = signed_headers(secret, body, "delivery-01");
        assert!(require_signature(secret, &headers, body).is_ok());
        assert!(matches!(
            require_signature("", &headers, body),
            Err(ApiError::Internal(_))
        ));
        assert!(matches!(
            require_signature(secret, &HeaderMap::new(), body),
            Err(ApiError::Unauthorized(_))
        ));
        assert!(matches!(
            require_signature(secret, &headers, b"changed"),
            Err(ApiError::Unauthorized(_))
        ));
        assert_eq!(delivery_id(&headers).unwrap(), "delivery-01");

        let mut both_aliases = headers.clone();
        both_aliases.insert("x-gitea-delivery", "delivery-01".parse().unwrap());
        assert_eq!(delivery_id(&both_aliases).unwrap(), "delivery-01");
        let mut gitea_only = both_aliases;
        gitea_only.remove("x-forgejo-delivery");
        assert_eq!(delivery_id(&gitea_only).unwrap(), "delivery-01");

        let mut event_aliases = headers.clone();
        event_aliases.insert("x-gitea-event", "PUSH".parse().unwrap());
        assert_eq!(event_name(&event_aliases).unwrap(), "push");
        event_aliases.insert("x-gitea-event", "issues".parse().unwrap());
        assert!(event_name(&event_aliases).is_err());
        let mut missing_event = headers.clone();
        missing_event.remove("x-forgejo-event");
        assert!(event_name(&missing_event).is_err());

        let mut missing = headers.clone();
        missing.remove("x-forgejo-delivery");
        assert!(delivery_id(&missing).is_err());
        let mut oversized = headers.clone();
        oversized.insert(
            "x-forgejo-delivery",
            "a".repeat(MAX_DELIVERY_ID_BYTES + 1).parse().unwrap(),
        );
        assert!(delivery_id(&oversized).is_err());
        let mut conflicting = headers;
        conflicting.insert("x-gitea-delivery", "different".parse().unwrap());
        assert!(delivery_id(&conflicting).is_err());

        assert!(valid_commit_sha(&"a".repeat(40)));
        assert!(valid_commit_sha(&"B".repeat(64)));
        assert!(valid_commit_sha(&"0".repeat(40)));
        assert!(!valid_commit_sha(""));
        assert!(!valid_commit_sha(&"a".repeat(39)));
        assert!(!valid_commit_sha(&"a".repeat(65)));
        assert!(!valid_commit_sha(&format!("{}g", "a".repeat(39))));
    }

    #[tokio::test]
    async fn concurrent_reservations_collapse_and_changed_payload_conflicts() {
        let Some(database_url) = database_url() else {
            return;
        };
        let pool = crate::init_db(&database_url).await.unwrap();
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let user_id = format!("webhook-user-{suffix}");
        let org_id = format!("webhook-org-{suffix}");
        let repo_id = format!("webhook-repo-id-{suffix}");
        let repo_name = format!("webhook-repo-{suffix}");
        sqlx::query("insert into users (id, name, email, display_name) values ($1, $1, '', $1)")
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into orgs (id, name, display_name, forgejo_owner, created_by) values ($1, $1, $1, 'clotho', $2)",
        )
        .bind(&org_id)
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("insert into repos (id, org_id, name, created_by) values ($1, $2, $3, $4)")
            .bind(&repo_id)
            .bind(&org_id)
            .bind(&repo_name)
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(control::get_repo_with_org(&pool, &repo_name)
            .await
            .unwrap()
            .is_some());

        let other_org_id = format!("webhook-other-org-{suffix}");
        let other_repo_id = format!("webhook-other-repo-id-{suffix}");
        sqlx::query(
            "insert into orgs (id, name, display_name, forgejo_owner, created_by) values ($1, $1, $1, 'clotho', $2)",
        )
        .bind(&other_org_id)
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("insert into repos (id, org_id, name, created_by) values ($1, $2, $3, $4)")
            .bind(&other_repo_id)
            .bind(&other_org_id)
            .bind(&repo_name)
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(control::get_repo_with_org(&pool, &repo_name)
            .await
            .unwrap()
            .is_none());
        sqlx::query("delete from repos where id = $1")
            .bind(&other_repo_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("delete from orgs where id = $1")
            .bind(&other_org_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(control::get_repo_with_org(&pool, &repo_name)
            .await
            .unwrap()
            .is_some());

        let raw_delivery = format!("delivery-{suffix}");
        let commit_sha = "0123456789abcdef0123456789abcdef01234567";
        let raw_payload = serde_json::to_vec(&serde_json::json!({
            "after": commit_sha,
            "repository": {"name": repo_name},
        }))
        .unwrap();
        let delivery_hash = sha256(raw_delivery.as_bytes());
        let payload_hash = sha256(&raw_payload);
        let left = reserve_delivery(
            &pool,
            &delivery_hash,
            &payload_hash,
            &org_id,
            &repo_id,
            "push",
            commit_sha,
        );
        let right = reserve_delivery(
            &pool,
            &delivery_hash,
            &payload_hash,
            &org_id,
            &repo_id,
            "push",
            commit_sha,
        );
        let (left, right) = tokio::join!(left, right);
        let outcomes = [left.unwrap(), right.unwrap()];
        assert!(outcomes.contains(&Reservation::New));
        assert!(outcomes.contains(&Reservation::Replay));

        let changed = reserve_delivery(
            &pool,
            &delivery_hash,
            &sha256(b"different payload"),
            &org_id,
            &repo_id,
            "push",
            commit_sha,
        )
        .await;
        assert!(matches!(changed, Err(ApiError::Conflict(_))));

        let row = sqlx::query(
            "select delivery_hash, payload_hash from webhook_deliveries where delivery_hash = $1",
        )
        .bind(&delivery_hash)
        .fetch_one(&pool)
        .await
        .unwrap();
        let stored_delivery: String = row.get("delivery_hash");
        let stored_payload: String = row.get("payload_hash");
        assert_eq!(stored_delivery.len(), 64);
        assert_eq!(stored_payload.len(), 64);
        assert_ne!(stored_delivery, raw_delivery);
        assert_ne!(stored_payload.as_bytes(), raw_payload.as_slice());

        sqlx::query(
            r#"update webhook_deliveries
               set created_at = now() - interval '2 days',
                   expires_at = now() - interval '1 day'
               where delivery_hash = $1"#,
        )
        .bind(&delivery_hash)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(cleanup_expired(&pool).await.unwrap(), 1);

        let mut transaction = pool.begin().await.unwrap();
        for index in 0..=CLEANUP_BATCH_SIZE {
            let expired_delivery = sha256(format!("expired-{suffix}-{index}").as_bytes());
            sqlx::query(
                r#"insert into webhook_deliveries
                   (delivery_hash, payload_hash, org_id, repo_id, event_type,
                    commit_sha, created_at, expires_at)
                   values ($1, $2, $3, $4, 'push', $5,
                           now() - interval '2 days', now() - interval '1 day')"#,
            )
            .bind(expired_delivery)
            .bind(&payload_hash)
            .bind(&org_id)
            .bind(&repo_id)
            .bind(commit_sha)
            .execute(&mut *transaction)
            .await
            .unwrap();
        }
        transaction.commit().await.unwrap();
        assert_eq!(
            cleanup_expired(&pool).await.unwrap(),
            CLEANUP_BATCH_SIZE as u64
        );
        let remaining: i64 = sqlx::query_scalar(
            "select count(*)::bigint from webhook_deliveries where repo_id = $1",
        )
        .bind(&repo_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, 1);
        assert_eq!(cleanup_expired(&pool).await.unwrap(), 1);

        sqlx::query("delete from repos where id = $1")
            .bind(&repo_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("delete from orgs where id = $1")
            .bind(&org_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("delete from users where id = $1")
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
