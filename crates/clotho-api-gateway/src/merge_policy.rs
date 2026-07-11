//! Clotho-owned merge policy — enforced at merge time on the edge (ADR-0017).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::Row;

use crate::auth;
use crate::control;
use crate::error::ApiError;
use crate::forgejo::{CommitStatusInfo, ReviewInfo};
use crate::AppState;

/// Per-repo merge gates stored in Postgres.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergePolicy {
    pub require_passing_actions: bool,
    pub block_merge_when_conflicted: bool,
    pub require_review_approvals: i32,
    pub protect_default_branch: bool,
    pub updated_at: String,
}

impl Default for MergePolicy {
    fn default() -> Self {
        Self {
            require_passing_actions: false,
            block_merge_when_conflicted: true,
            require_review_approvals: 0,
            protect_default_branch: false,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateMergePolicyRequest {
    #[serde(default)]
    pub require_passing_actions: Option<bool>,
    #[serde(default)]
    pub block_merge_when_conflicted: Option<bool>,
    #[serde(default)]
    pub require_review_approvals: Option<i32>,
    #[serde(default)]
    pub protect_default_branch: Option<bool>,
}

/// Pure policy decision: given inputs, return Ok or a human-readable block reason.
pub fn evaluate_merge_policy(
    policy: &MergePolicy,
    mergeable: bool,
    statuses: &[CommitStatusInfo],
    approval_count: i64,
) -> Result<(), String> {
    if policy.block_merge_when_conflicted && !mergeable {
        return Err(
            "pull request has merge conflicts with the base branch — resolve conflicts before merging"
                .into(),
        );
    }

    if policy.require_passing_actions {
        if statuses.is_empty() {
            return Err("actions have not reported status on the pull request head commit".into());
        }
        for status in statuses {
            let state = status.state.to_ascii_lowercase();
            if matches!(state.as_str(), "failure" | "error") {
                let ctx = if status.context.is_empty() {
                    "check".into()
                } else {
                    status.context.clone()
                };
                return Err(format!(
                    "check {ctx} reported {state} — all actions must pass before merge"
                ));
            }
            if state == "pending" {
                let ctx = if status.context.is_empty() {
                    "check".into()
                } else {
                    status.context.clone()
                };
                return Err(format!(
                    "check {ctx} is still pending — wait for actions to finish before merge"
                ));
            }
        }
    }

    if policy.require_review_approvals > 0
        && approval_count < i64::from(policy.require_review_approvals)
    {
        return Err(format!(
            "requires {} approving review(s), found {}",
            policy.require_review_approvals, approval_count
        ));
    }

    Ok(())
}

/// Count distinct reviewers whose latest submitted review is APPROVED.
pub fn count_approving_reviews(reviews: &[ReviewInfo]) -> i64 {
    let mut seen = std::collections::HashSet::new();
    let mut count = 0i64;
    for review in reviews {
        if !seen.insert(review.user.login.clone()) {
            continue;
        }
        if review.state.eq_ignore_ascii_case("APPROVED") {
            count += 1;
        }
    }
    count
}

pub async fn load_merge_policy(pool: &PgPool, repo_id: &str) -> Result<MergePolicy, ApiError> {
    let row = sqlx::query(
        r#"
        select require_passing_actions, block_merge_when_conflicted,
               require_review_approvals, protect_default_branch, updated_at
        from repo_merge_policies
        where repo_id = $1
        "#,
    )
    .bind(repo_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("load merge policy: {e}")))?;

    Ok(match row {
        Some(row) => MergePolicy {
            require_passing_actions: row.get("require_passing_actions"),
            block_merge_when_conflicted: row.get("block_merge_when_conflicted"),
            require_review_approvals: row.get("require_review_approvals"),
            protect_default_branch: row.get("protect_default_branch"),
            updated_at: row.get::<DateTime<Utc>, _>("updated_at").to_rfc3339(),
        },
        None => MergePolicy::default(),
    })
}

pub async fn upsert_merge_policy(
    pool: &PgPool,
    repo_id: &str,
    req: &UpdateMergePolicyRequest,
) -> Result<MergePolicy, ApiError> {
    let current = load_merge_policy(pool, repo_id).await?;
    let require_passing_actions = req
        .require_passing_actions
        .unwrap_or(current.require_passing_actions);
    let block_merge_when_conflicted = req
        .block_merge_when_conflicted
        .unwrap_or(current.block_merge_when_conflicted);
    let require_review_approvals = req
        .require_review_approvals
        .unwrap_or(current.require_review_approvals);
    let protect_default_branch = req
        .protect_default_branch
        .unwrap_or(current.protect_default_branch);

    if require_review_approvals < 0 {
        return Err(ApiError::InvalidRequest(
            "require_review_approvals must be >= 0".into(),
        ));
    }

    let row = sqlx::query(
        r#"
        insert into repo_merge_policies (
            repo_id, require_passing_actions, block_merge_when_conflicted,
            require_review_approvals, protect_default_branch, updated_at
        ) values ($1, $2, $3, $4, $5, now())
        on conflict (repo_id) do update set
            require_passing_actions = excluded.require_passing_actions,
            block_merge_when_conflicted = excluded.block_merge_when_conflicted,
            require_review_approvals = excluded.require_review_approvals,
            protect_default_branch = excluded.protect_default_branch,
            updated_at = now()
        returning require_passing_actions, block_merge_when_conflicted,
                  require_review_approvals, protect_default_branch, updated_at
        "#,
    )
    .bind(repo_id)
    .bind(require_passing_actions)
    .bind(block_merge_when_conflicted)
    .bind(require_review_approvals)
    .bind(protect_default_branch)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("upsert merge policy: {e}")))?;

    Ok(MergePolicy {
        require_passing_actions: row.get("require_passing_actions"),
        block_merge_when_conflicted: row.get("block_merge_when_conflicted"),
        require_review_approvals: row.get("require_review_approvals"),
        protect_default_branch: row.get("protect_default_branch"),
        updated_at: row.get::<DateTime<Utc>, _>("updated_at").to_rfc3339(),
    })
}

pub async fn get_merge_policy_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<MergePolicy>, ApiError> {
    let repo = auth::require_repo_read(&headers, &state, &name).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("merge policy requires postgres".into()))?;
    let policy = load_merge_policy(pool, &repo.repo.id).await?;
    Ok(Json(policy))
}

pub async fn put_merge_policy_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(req): Json<UpdateMergePolicyRequest>,
) -> Result<Json<MergePolicy>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("merge policy requires postgres".into()))?;
    control::require_repo_permission(pool, &name, &auth.user_id, "admin").await?;
    let repo = control::get_repo_by_name(pool, &name).await?;
    let policy = upsert_merge_policy(pool, &repo.id, &req).await?;
    Ok(Json(policy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forgejo::{CommitStatusInfo, PullUser, ReviewInfo};

    fn status(state: &str, context: &str) -> CommitStatusInfo {
        CommitStatusInfo {
            id: 1,
            state: state.into(),
            context: context.into(),
            description: String::new(),
            target_url: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn review(login: &str, state: &str) -> ReviewInfo {
        ReviewInfo {
            id: 1,
            body: String::new(),
            user: PullUser {
                login: login.into(),
            },
            state: state.into(),
            html_url: String::new(),
            submitted_at: String::new(),
        }
    }

    #[test]
    fn blocks_conflicted_by_default() {
        let policy = MergePolicy::default();
        let err = evaluate_merge_policy(&policy, false, &[], 0).unwrap_err();
        assert!(err.contains("conflict"));
    }

    #[test]
    fn allows_mergeable_with_defaults() {
        let policy = MergePolicy::default();
        assert!(evaluate_merge_policy(&policy, true, &[], 0).is_ok());
    }

    #[test]
    fn requires_passing_actions() {
        let policy = MergePolicy {
            require_passing_actions: true,
            ..MergePolicy::default()
        };
        assert!(evaluate_merge_policy(&policy, true, &[], 0)
            .unwrap_err()
            .contains("not reported"));
        assert!(
            evaluate_merge_policy(&policy, true, &[status("failure", "clotho/actions")], 0)
                .unwrap_err()
                .contains("failure")
        );
        assert!(
            evaluate_merge_policy(&policy, true, &[status("pending", "clotho/actions")], 0)
                .unwrap_err()
                .contains("pending")
        );
        assert!(
            evaluate_merge_policy(&policy, true, &[status("success", "clotho/actions")], 0).is_ok()
        );
    }

    #[test]
    fn requires_review_approvals() {
        let policy = MergePolicy {
            require_review_approvals: 2,
            ..MergePolicy::default()
        };
        assert!(evaluate_merge_policy(&policy, true, &[], 1)
            .unwrap_err()
            .contains("requires 2"));
        assert!(evaluate_merge_policy(&policy, true, &[], 2).is_ok());
    }

    #[test]
    fn counts_latest_approval_per_reviewer() {
        let reviews = vec![
            review("alice", "APPROVED"),
            review("alice", "REQUEST_CHANGES"),
            review("bob", "APPROVED"),
        ];
        assert_eq!(count_approving_reviews(&reviews), 2);
    }
}
