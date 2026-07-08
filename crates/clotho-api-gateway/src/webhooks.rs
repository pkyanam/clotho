//! Forgejo push-webhook receiver → Stage 7 CI (docs/adr/0008).
//!
//! Forgejo POSTs here on every push (the hook is registered per repo at
//! creation time). We verify the HMAC-SHA256 signature, then spawn the CI run
//! detached and return `202` immediately — the check runs asynchronously and
//! reports status back to the commit via `ci::run`.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use crate::{ci, AppState};

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

/// Handle a Forgejo webhook delivery. Always returns quickly; CI runs detached.
pub async fn forgejo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    // Only push events start CI; ignore everything else with a 200.
    let event = header(&headers, "x-gitea-event")
        .or_else(|| header(&headers, "x-forgejo-event"))
        .unwrap_or_default();
    if !event.is_empty() && event != "push" {
        return ignored(format!("event {event:?} ignored"));
    }

    // Verify the signature when a secret is configured. An unsigned or
    // mismatched delivery is rejected; a missing secret (dev) skips the check.
    if !state.webhook_secret.is_empty() {
        let sig = header(&headers, "x-gitea-signature")
            .or_else(|| header(&headers, "x-forgejo-signature"))
            .or_else(|| {
                header(&headers, "x-hub-signature-256")
                    .map(|s| s.trim_start_matches("sha256=").to_string())
            });
        match sig {
            Some(sig) if verify(&state.webhook_secret, &body, &sig) => {}
            _ => {
                tracing::warn!("rejected webhook: bad or missing signature");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "invalid signature" })),
                );
            }
        }
    }

    let payload: PushPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("bad payload: {e}") })),
            );
        }
    };

    let repo = payload.repository.name;
    let sha = payload.after;
    if repo.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "missing repository name" })),
        );
    }
    // Branch deletion (all-zero sha) has nothing to check.
    if sha.chars().all(|c| c == '0') {
        return ignored("branch deletion, nothing to check".to_string());
    }
    if !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid commit sha" })),
        );
    }

    tracing::info!(%repo, %sha, "push webhook accepted; starting ci");
    tokio::spawn(ci::run(state, repo.clone(), sha.clone()));
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "status": "accepted", "repo": repo, "sha": sha })),
    )
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
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Constant-time HMAC-SHA256 check of the raw body against a hex signature.
fn verify(secret: &str, body: &[u8], sig_hex: &str) -> bool {
    let Ok(expected) = decode_hex(sig_hex) else {
        return false;
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac accepts any key");
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ()> {
    if !s.len().is_multiple_of(2) {
        return Err(());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| ()))
        .collect()
}
