//! Clotho-owned notification feed (Slice D). Postgres-backed; polled by the web
//! app — no websockets in the prototype.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::auth;
use crate::error::ApiError;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    pub id: i64,
    pub user_id: String,
    pub repo_name: Option<String>,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub href: String,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct NotificationListResponse {
    pub notifications: Vec<Notification>,
    pub unread_count: i64,
}

#[derive(Deserialize)]
pub struct NotificationQuery {
    #[serde(default)]
    pub unread: bool,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

#[derive(Deserialize)]
pub struct MarkReadRequest {
    #[serde(default)]
    pub ids: Vec<i64>,
    #[serde(default)]
    pub all: bool,
}

/// Insert a notification for a Clotho user.
pub async fn notify_user(
    pool: &PgPool,
    user_id: &str,
    kind: &str,
    title: &str,
    body: &str,
    href: &str,
    repo_name: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into notifications (user_id, repo_name, kind, title, body, href)
        values ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(user_id)
    .bind(repo_name)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(href)
    .execute(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("notify user: {e}")))?;
    Ok(())
}

/// Map a collaboration login (assignee/author) to a Clotho user id by name.
pub async fn user_id_by_name(pool: &PgPool, name: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>("select id from users where name = $1 limit 1")
        .bind(name.trim())
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

/// Best-effort `@name` mentions in comment bodies (lowercase usernames only).
pub fn parse_mentions(body: &str) -> Vec<String> {
    let mut names = Vec::new();
    for word in body.split_whitespace() {
        if let Some(rest) = word.strip_prefix('@') {
            let name: String = rest
                .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                .to_string();
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// Notify assignees that map to Clotho users (skips the actor).
pub async fn notify_issue_assigned(
    pool: &PgPool,
    repo_name: &str,
    issue_number: i64,
    issue_title: &str,
    assignees: &[String],
    actor_name: Option<&str>,
) {
    let href = format!("/repos/{repo_name}/issues/{issue_number}");
    let title = format!("assigned to issue #{issue_number}");
    let body = issue_title.to_string();
    for login in assignees {
        if actor_name.is_some_and(|a| a == login) {
            continue;
        }
        if let Some(user_id) = user_id_by_name(pool, login).await {
            let _ = notify_user(
                pool,
                &user_id,
                "issue_assigned",
                &title,
                &body,
                &href,
                Some(repo_name),
            )
            .await;
        }
    }
}

/// Notify issue author on a new comment (if different from commenter).
pub async fn notify_issue_comment(
    pool: &PgPool,
    repo_name: &str,
    issue_number: i64,
    issue_title: &str,
    author_login: &str,
    commenter_login: &str,
    comment_body: &str,
) {
    if author_login == commenter_login {
        return;
    }
    if let Some(user_id) = user_id_by_name(pool, author_login).await {
        let href = format!("/repos/{repo_name}/issues/{issue_number}");
        let title = format!("comment on issue #{issue_number}");
        let preview: String = comment_body.chars().take(200).collect();
        let _ = notify_user(
            pool,
            &user_id,
            "comment",
            &title,
            &preview,
            &href,
            Some(repo_name),
        )
        .await;
    }
    // Best-effort mention notifications.
    for mention in parse_mentions(comment_body) {
        if mention == commenter_login || mention == author_login {
            continue;
        }
        if let Some(user_id) = user_id_by_name(pool, &mention).await {
            let href = format!("/repos/{repo_name}/issues/{issue_number}");
            let title = format!("mentioned in issue #{issue_number}");
            let _ = notify_user(
                pool,
                &user_id,
                "mention",
                &title,
                issue_title,
                &href,
                Some(repo_name),
            )
            .await;
        }
    }
}

/// Notify repo creator (or bootstrap) when an action run fails.
pub async fn notify_action_failed(
    pool: &PgPool,
    repo_name: &str,
    run_id: &str,
    conclusion: &str,
    notify_user_id: &str,
) {
    if conclusion != "failure" && conclusion != "failed" {
        return;
    }
    let href = format!("/repos/{repo_name}/actions/runs/{run_id}");
    let title = format!("action run failed on {repo_name}");
    let body = format!("run {run_id} concluded with {conclusion}");
    let _ = notify_user(
        pool,
        notify_user_id,
        "action_failed",
        &title,
        &body,
        &href,
        Some(repo_name),
    )
    .await;
}

pub async fn list_notifications_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<NotificationQuery>,
) -> Result<Json<NotificationListResponse>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("postgres not configured".into()))?;
    let limit = query.limit.clamp(1, 100);

    let notifications = if query.unread {
        sqlx::query_as::<_, NotificationRow>(
            r#"
            select id, user_id, repo_name, kind, title, body, href, read_at, created_at
            from notifications
            where user_id = $1 and read_at is null
            order by created_at desc
            limit $2
            "#,
        )
        .bind(&auth.user_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, NotificationRow>(
            r#"
            select id, user_id, repo_name, kind, title, body, href, read_at, created_at
            from notifications
            where user_id = $1
            order by created_at desc
            limit $2
            "#,
        )
        .bind(&auth.user_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }
    .map_err(|e| ApiError::Internal(format!("list notifications: {e}")))?
    .into_iter()
    .map(NotificationRow::into_notification)
    .collect();

    let unread_count: i64 = sqlx::query_scalar(
        "select count(*)::bigint from notifications where user_id = $1 and read_at is null",
    )
    .bind(&auth.user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError::Internal(format!("count notifications: {e}")))?;

    Ok(Json(NotificationListResponse {
        notifications,
        unread_count,
    }))
}

pub async fn mark_read_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<MarkReadRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::Internal("postgres not configured".into()))?;

    if req.all {
        sqlx::query(
            "update notifications set read_at = now() where user_id = $1 and read_at is null",
        )
        .bind(&auth.user_id)
        .execute(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("mark all read: {e}")))?;
    } else if !req.ids.is_empty() {
        sqlx::query(
            r#"
            update notifications set read_at = now()
            where user_id = $1 and id = any($2) and read_at is null
            "#,
        )
        .bind(&auth.user_id)
        .bind(&req.ids)
        .execute(pool)
        .await
        .map_err(|e| ApiError::Internal(format!("mark read: {e}")))?;
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(sqlx::FromRow)]
struct NotificationRow {
    id: i64,
    user_id: String,
    repo_name: Option<String>,
    kind: String,
    title: String,
    body: String,
    href: String,
    read_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl NotificationRow {
    fn into_notification(self) -> Notification {
        Notification {
            id: self.id,
            user_id: self.user_id,
            repo_name: self.repo_name,
            kind: self.kind,
            title: self.title,
            body: self.body,
            href: self.href,
            read_at: self.read_at,
            created_at: self.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_mentions;

    #[test]
    fn parse_mentions_finds_at_names() {
        let names = parse_mentions("hey @alice and @bob-dev check this");
        assert_eq!(names, vec!["alice".to_string(), "bob-dev".to_string()]);
    }

    #[test]
    fn parse_mentions_skips_duplicates() {
        let names = parse_mentions("@alice @alice");
        assert_eq!(names, vec!["alice".to_string()]);
    }
}
