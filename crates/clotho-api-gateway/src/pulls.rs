//! Pull-request endpoints: list/detail proxied from Forgejo (the
//! collaboration shell owns PRs — ADR-0003), plus the structured PR diff:
//! clotho-vcs `DiffCommits` for contents, clotho-diff `DiffFiles` for
//! symbol-level structure, and line hunks computed here at the edge. One
//! structured-diff object feeds both the agent API and this human view
//! (docs/prd.md §2); unresolved jj conflicts arrive materialized and
//! flagged, never hidden (ADR-0006).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use clotho_common::pb::diff::v1::{ChangeStatus, DiffFilesRequest, FileDiffInput, SymbolChange};
use clotho_common::pb::vcs::v1::changed_file::ChangeKind;
use clotho_common::pb::vcs::v1::DiffCommitsRequest;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::forgejo::PullInfo;
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

pub async fn get_pull(
    State(state): State<Arc<AppState>>,
    Path((name, number)): Path<(String, i64)>,
) -> Result<Json<PullInfo>, ApiError> {
    Ok(Json(state.forgejo.get_pull(&name, number).await?))
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
