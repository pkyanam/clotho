//! Pull-request endpoints: list/detail proxied from Forgejo (the
//! collaboration shell owns PRs — ADR-0003), plus the structured PR diff:
//! clotho-vcs `DiffCommits` for contents, clotho-diff `DiffFiles` for
//! symbol-level structure, and line hunks computed here at the edge. One
//! structured-diff object feeds both the agent API and this human view
//! (docs/prd.md §2); unresolved jj conflicts arrive materialized and
//! flagged, never hidden (ADR-0006).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use clotho_common::pb::diff::v1::{ChangeStatus, DiffFilesRequest, FileDiffInput, SymbolChange};
use clotho_common::pb::vcs::v1::changed_file::ChangeKind;
use clotho_common::pb::vcs::v1::DiffCommitsRequest;
use serde::{Deserialize, Serialize};

use crate::auth;
use crate::control;
use crate::error::ApiError;
use crate::forgejo::{CommentInfo, PullInfo, ReviewInfo};
use crate::merge_policy::{
    count_approving_reviews, evaluate_merge_policy, load_merge_policy, MergePolicy,
};
use crate::AppState;

#[derive(Deserialize)]
pub struct PullsQuery {
    #[serde(default = "default_pull_state")]
    pub state: String,
}

fn default_pull_state() -> String {
    "all".into()
}

#[derive(Serialize)]
pub struct PullListResponse {
    pub pulls: Vec<PullInfo>,
}

#[derive(Deserialize)]
pub struct CreatePullRequest {
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub head: String,
    #[serde(default = "default_base_branch")]
    pub base: String,
}

fn default_base_branch() -> String {
    "main".into()
}

#[derive(Deserialize)]
pub struct PullCommentRequest {
    pub body: String,
    #[serde(default)]
    pub in_reply_to: Option<i64>,
}

#[derive(Deserialize)]
pub struct PullReviewRequest {
    #[serde(default)]
    pub body: String,
    #[serde(default = "default_review_event")]
    pub event: String,
}

fn default_review_event() -> String {
    "COMMENT".into()
}

#[derive(Deserialize)]
pub struct PullMergeRequest {
    #[serde(default = "default_merge_method")]
    pub method: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

fn default_merge_method() -> String {
    "merge".into()
}

pub async fn list_pulls(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<PullsQuery>,
) -> Result<Json<PullListResponse>, ApiError> {
    if !matches!(query.state.as_str(), "open" | "closed" | "all") {
        return Err(ApiError::InvalidRequest(format!(
            "state {:?} must be open, closed, or all",
            query.state
        )));
    }
    let pulls = state.forgejo.list_pulls(&name, &query.state).await?;
    Ok(Json(PullListResponse { pulls }))
}

pub async fn create_pull(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(req): Json<CreatePullRequest>,
) -> Result<(StatusCode, Json<PullInfo>), ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    if let Some(pool) = &state.pool {
        control::require_repo_permission(pool, &name, &auth.user_id, "write").await?;
    }
    if req.title.trim().is_empty() {
        return Err(ApiError::InvalidRequest("title is required".into()));
    }
    if req.head.trim().is_empty() {
        return Err(ApiError::InvalidRequest("head is required".into()));
    }
    let pull = state
        .forgejo
        .create_pull(
            &name,
            req.title.trim(),
            req.body.trim(),
            req.head.trim(),
            req.base.trim(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(pull)))
}

pub async fn get_pull(
    State(state): State<Arc<AppState>>,
    Path((name, number)): Path<(String, i64)>,
) -> Result<Json<PullInfo>, ApiError> {
    Ok(Json(state.forgejo.get_pull(&name, number).await?))
}

#[derive(Serialize)]
pub struct PullCommentListResponse {
    pub comments: Vec<CommentInfo>,
}

/// List pull comments — prefers inline review comments with threading metadata,
/// and includes flat issue-style discussion comments when present.
pub async fn list_pull_comments(
    State(state): State<Arc<AppState>>,
    Path((name, number)): Path<(String, i64)>,
) -> Result<Json<PullCommentListResponse>, ApiError> {
    let mut comments = state
        .forgejo
        .list_pull_review_comments(&name, number)
        .await
        .unwrap_or_default();
    let issue_comments = state.forgejo.list_issue_comments(&name, number).await?;
    for comment in issue_comments {
        if !comments.iter().any(|c| c.id == comment.id) {
            comments.push(comment);
        }
    }
    Ok(Json(PullCommentListResponse { comments }))
}

#[derive(Serialize)]
pub struct PullReviewListResponse {
    pub reviews: Vec<ReviewInfo>,
}

pub async fn list_pull_reviews(
    State(state): State<Arc<AppState>>,
    Path((name, number)): Path<(String, i64)>,
) -> Result<Json<PullReviewListResponse>, ApiError> {
    let reviews = state.forgejo.list_pull_reviews(&name, number).await?;
    Ok(Json(PullReviewListResponse { reviews }))
}

pub async fn comment_on_pull(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, number)): Path<(String, i64)>,
    Json(req): Json<PullCommentRequest>,
) -> Result<(StatusCode, Json<CommentInfo>), ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    if let Some(pool) = &state.pool {
        control::require_repo_permission(pool, &name, &auth.user_id, "write").await?;
    }
    if req.body.trim().is_empty() {
        return Err(ApiError::InvalidRequest("body is required".into()));
    }
    let comment = if let Some(reply_to) = req.in_reply_to.filter(|id| *id > 0) {
        state
            .forgejo
            .reply_on_pull_comment(&name, number, req.body.trim(), reply_to)
            .await?
    } else {
        state
            .forgejo
            .comment_on_pull(&name, number, req.body.trim())
            .await?
    };
    Ok((StatusCode::CREATED, Json(comment)))
}

pub async fn review_pull(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, number)): Path<(String, i64)>,
    Json(req): Json<PullReviewRequest>,
) -> Result<(StatusCode, Json<CommentInfo>), ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    if let Some(pool) = &state.pool {
        control::require_repo_permission(pool, &name, &auth.user_id, "write").await?;
    }
    let event = req.event.trim().to_ascii_uppercase();
    if !matches!(event.as_str(), "COMMENT" | "APPROVE" | "REQUEST_CHANGES") {
        return Err(ApiError::InvalidRequest(
            "event must be COMMENT, APPROVE, or REQUEST_CHANGES".into(),
        ));
    }
    let comment = state
        .forgejo
        .review_pull(&name, number, req.body.trim(), &event)
        .await?;
    Ok((StatusCode::CREATED, Json(comment)))
}

pub async fn merge_pull(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, number)): Path<(String, i64)>,
    Json(req): Json<PullMergeRequest>,
) -> Result<Json<PullInfo>, ApiError> {
    let auth = auth::resolve_auth(&headers, &state).await?;
    if let Some(pool) = &state.pool {
        control::require_repo_permission(pool, &name, &auth.user_id, "write").await?;
    }
    let method = req.method.trim();
    if !matches!(method, "merge" | "rebase" | "rebase-merge" | "squash") {
        return Err(ApiError::InvalidRequest(
            "method must be merge, rebase, rebase-merge, or squash".into(),
        ));
    }

    let pull = state.forgejo.get_pull(&name, number).await?;
    let policy = merge_policy_for_repo(&state, &name).await?;
    let statuses = state
        .forgejo
        .commit_statuses(&name, &pull.head.sha)
        .await
        .unwrap_or_default();
    let reviews = state
        .forgejo
        .list_pull_reviews(&name, number)
        .await
        .unwrap_or_default();
    let approvals = count_approving_reviews(&reviews);
    if let Err(reason) = evaluate_merge_policy(&policy, pull.mergeable, &statuses, approvals) {
        return Err(ApiError::Conflict(reason));
    }

    let pull = state
        .forgejo
        .merge_pull(
            &name,
            number,
            method,
            req.title.as_deref(),
            req.message.as_deref(),
        )
        .await?;
    Ok(Json(pull))
}

async fn merge_policy_for_repo(state: &AppState, name: &str) -> Result<MergePolicy, ApiError> {
    match &state.pool {
        Some(pool) => {
            let repo = control::get_repo_by_name(pool, name).await?;
            load_merge_policy(pool, &repo.id).await
        }
        None => Ok(MergePolicy::default()),
    }
}

/// One line of a diff hunk. Line numbers are 1-based; absent on the side a
/// line does not exist on.
#[derive(Serialize)]
pub struct DiffLineJson {
    /// "context" | "add" | "del"
    pub kind: &'static str,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub text: String,
}

#[derive(Serialize)]
pub struct HunkJson {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLineJson>,
}

#[derive(Serialize)]
pub struct SymbolJson {
    pub name: String,
    pub kind: String,
    /// "added" | "modified" | "removed"
    pub status: &'static str,
    pub old_start_line: u32,
    pub old_end_line: u32,
    pub new_start_line: u32,
    pub new_end_line: u32,
}

#[derive(Serialize)]
pub struct FileDiffJson {
    pub path: String,
    /// "added" | "modified" | "deleted"
    pub status: &'static str,
    /// Detected language ("rust", "typescript"); empty when unsupported.
    pub language: String,
    /// The new side is an unresolved jj conflict; the hunks then contain
    /// its materialized conflict-marker text (ADR-0006).
    pub conflicted: bool,
    pub binary: bool,
    pub symbols: Vec<SymbolJson>,
    pub hunks: Vec<HunkJson>,
}

#[derive(Serialize)]
pub struct PullDiffResponse {
    pub from_commit_id: String,
    pub to_commit_id: String,
    /// Any file in this diff is an unresolved conflict.
    pub conflicted: bool,
    pub files: Vec<FileDiffJson>,
}

pub async fn pull_diff(
    State(state): State<Arc<AppState>>,
    Path((name, number)): Path<(String, i64)>,
) -> Result<Json<PullDiffResponse>, ApiError> {
    let pull = state.forgejo.get_pull(&name, number).await?;
    // Diff what the PR introduces: merge base → head (three-dot semantics);
    // fall back to the base branch head if Forgejo omitted the merge base.
    let from = if pull.merge_base.is_empty() {
        pull.base.sha.clone()
    } else {
        pull.merge_base.clone()
    };

    let mut vcs = state.vcs.clone();
    let changes = vcs
        .diff_commits(DiffCommitsRequest {
            repo: name.clone(),
            from_commit_id: from,
            to_commit_id: pull.head.sha.clone(),
        })
        .await?
        .into_inner();

    let mut diff = state.diff.clone();
    let structured = diff
        .diff_files(DiffFilesRequest {
            files: changes
                .files
                .iter()
                .map(|f| FileDiffInput {
                    path: f.path.clone(),
                    old_content: f.old_content.clone(),
                    new_content: f.new_content.clone(),
                })
                .collect(),
        })
        .await
        .map_err(|status| ApiError::Upstream(format!("clotho-diff: {}", status.message())))?
        .into_inner();

    let mut files = Vec::with_capacity(changes.files.len());
    for changed in changes.files {
        let symbols = structured
            .files
            .iter()
            .find(|f| f.path == changed.path)
            .map(|f| {
                (
                    f.language.clone(),
                    f.symbols.iter().map(symbol_json).collect::<Vec<_>>(),
                )
            });
        let (language, symbols) = symbols.unwrap_or_default();
        let status = match changed.kind() {
            ChangeKind::Added => "added",
            ChangeKind::Deleted => "deleted",
            _ => "modified",
        };
        let old_text = String::from_utf8(changed.old_content).ok();
        let new_text = String::from_utf8(changed.new_content).ok();
        let binary = old_text.is_none() || new_text.is_none();
        let hunks = match (&old_text, &new_text) {
            (Some(old), Some(new)) => line_hunks(old, new),
            _ => Vec::new(),
        };
        files.push(FileDiffJson {
            path: changed.path,
            status,
            language,
            conflicted: changed.conflicted,
            binary,
            symbols,
            hunks,
        });
    }

    Ok(Json(PullDiffResponse {
        from_commit_id: changes.from_commit_id,
        to_commit_id: changes.to_commit_id,
        conflicted: files.iter().any(|f| f.conflicted),
        files,
    }))
}

fn symbol_json(s: &SymbolChange) -> SymbolJson {
    SymbolJson {
        name: s.name.clone(),
        kind: s.kind.clone(),
        status: match s.status() {
            ChangeStatus::Added => "added",
            ChangeStatus::Removed => "removed",
            _ => "modified",
        },
        old_start_line: s.old_start_line,
        old_end_line: s.old_end_line,
        new_start_line: s.new_start_line,
        new_end_line: s.new_end_line,
    }
}

/// Unified-diff-style hunks (3 lines of context) over two text files.
fn line_hunks(old: &str, new: &str) -> Vec<HunkJson> {
    use similar::{ChangeTag, TextDiff};

    let diff = TextDiff::from_lines(old, new);
    let mut hunks = Vec::new();
    for group in diff.grouped_ops(3) {
        let mut lines = Vec::new();
        for op in &group {
            for change in diff.iter_changes(op) {
                lines.push(DiffLineJson {
                    kind: match change.tag() {
                        ChangeTag::Equal => "context",
                        ChangeTag::Insert => "add",
                        ChangeTag::Delete => "del",
                    },
                    old_line: change.old_index().map(|i| i as u32 + 1),
                    new_line: change.new_index().map(|i| i as u32 + 1),
                    text: change.value().trim_end_matches('\n').to_string(),
                });
            }
        }
        hunks.push(HunkJson {
            old_start: lines.iter().find_map(|l| l.old_line).unwrap_or(0),
            old_lines: lines.iter().filter(|l| l.old_line.is_some()).count() as u32,
            new_start: lines.iter().find_map(|l| l.new_line).unwrap_or(0),
            new_lines: lines.iter().filter(|l| l.new_line.is_some()).count() as u32,
            lines,
        });
    }
    hunks
}
