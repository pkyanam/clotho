//! The MCP tool surface: VCS tools over gRPC plus Stage 15 collab/Actions/
//! platform tools over the api-gateway REST edge — guarded by scoped agent
//! tokens, with every invocation audited.

use clotho_common::pb::diff::v1::diff_client::DiffClient;
use clotho_common::pb::diff::v1::{ChangeStatus, DiffFilesRequest, FileDiffInput};
use clotho_common::pb::mergequeue::v1::merge_queue_client::MergeQueueClient;
use clotho_common::pb::mergequeue::v1::SubmitChangeRequest;
use clotho_common::pb::vcs::v1::vcs_client::VcsClient;
use clotho_common::pb::vcs::v1::{
    CheckpointRequest, CommitRequest, DiffCommitsRequest, FileChange, GetHeadsRequest,
    ListFilesRequest, QueryOpLogRequest, RestoreToRequest,
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::handler::server::{router::tool::ToolRouter, tool::ToolCallContext};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ErrorData as McpError, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_router, RoleServer, ServerHandler};
use serde_json::{json, Value};
use tonic::transport::Channel;

use crate::identity::{sha256, AuthedAgent, ForwardedAgentBearer, IdentityStore};
use crate::rest::{with_forwarded_agent_bearer, RestClient};

/// Errors a tool body can produce: upstream gRPC failures (mapped to plain
/// MCP internal/invalid-params errors) or already-shaped MCP errors. Local
/// enum because the orphan rule forbids `From<tonic::Status> for ErrorData`.
pub enum ToolError {
    Grpc(tonic::Status),
    Mcp(McpError),
}

impl From<tonic::Status> for ToolError {
    fn from(status: tonic::Status) -> Self {
        Self::Grpc(status)
    }
}

impl From<McpError> for ToolError {
    fn from(err: McpError) -> Self {
        Self::Mcp(err)
    }
}

impl From<ToolError> for McpError {
    fn from(err: ToolError) -> Self {
        match err {
            ToolError::Grpc(status) => match status.code() {
                tonic::Code::InvalidArgument | tonic::Code::NotFound => {
                    McpError::invalid_params(status.message().to_string(), None)
                }
                _ => McpError::internal_error(status.message().to_string(), None),
            },
            ToolError::Mcp(err) => err,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct CheckpointParams {
    /// Repository name.
    pub repo: String,
    /// Human/agent-readable label, e.g. "before refactoring auth".
    pub label: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct RestoreToParams {
    /// Repository name.
    pub repo: String,
    /// Operation id to restore to — from a `checkpoint` result or the
    /// op log in `orient_repo`.
    pub operation_id: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct OrientRepoParams {
    /// Repository name.
    pub repo: String,
    /// Maximum op-log entries to return (default 20, 0 = unlimited).
    pub op_log_limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct DiffSymbolParams {
    /// Repository name.
    pub repo: String,
    /// Base commit id; defaults to the first parent of `to_commit_id`.
    pub from_commit_id: Option<String>,
    /// Target commit id; defaults to the current `main` head.
    pub to_commit_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct CommitFileParam {
    /// Repo-relative path, e.g. "src/lib.rs".
    pub path: String,
    /// UTF-8/text file contents. Binary payloads are deferred to a later
    /// artifact-aware tool surface.
    pub content: String,
    /// Whether the file should be executable.
    pub executable: Option<bool>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct CommitParams {
    /// Repository name.
    pub repo: String,
    /// Commit message.
    pub message: String,
    /// Files to add or replace.
    pub files: Vec<CommitFileParam>,
    /// Repo-relative paths to delete.
    pub deleted_paths: Option<Vec<String>>,
    /// Explicit parent commit ids. Omit to commit on the current head(s).
    pub parent_commit_ids: Option<Vec<String>>,
    /// Display author name. Defaults to the authenticated agent name.
    pub author_name: Option<String>,
    /// Author email. Defaults to `<agent>@agents.clotho.internal`.
    pub author_email: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct SubmitChangeParams {
    /// Repository name.
    pub repo: String,
    /// Commit id returned by `commit`.
    pub commit_id: String,
}

// ---------------------------------------------------------------------------
// Stage 15 collab / Actions / platform params (REST-backed)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct ListIssuesParams {
    pub repo: String,
    /// open | closed | all (default open)
    pub state: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct CreateIssueParams {
    pub repo: String,
    pub title: String,
    pub body: Option<String>,
    /// Label names to apply on create.
    pub labels: Option<Vec<String>>,
    /// Assignee logins.
    pub assignees: Option<Vec<String>>,
    /// Milestone id.
    pub milestone: Option<i64>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct CommentIssueParams {
    pub repo: String,
    pub number: i64,
    pub body: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct ListPullsParams {
    pub repo: String,
    /// open | closed | all (default all)
    pub state: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct CreatePullParams {
    pub repo: String,
    pub title: String,
    pub head: String,
    pub base: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct CommentPullParams {
    pub repo: String,
    pub number: i64,
    pub body: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct ReviewPullParams {
    pub repo: String,
    pub number: i64,
    /// COMMENT | APPROVE | REQUEST_CHANGES
    pub event: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct MergePullParams {
    pub repo: String,
    pub number: i64,
    /// merge | rebase | rebase-merge | squash
    pub method: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct ListActionRunsParams {
    pub repo: String,
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct StartActionRunParams {
    pub repo: String,
    pub commit_id: Option<String>,
    pub branch: Option<String>,
    pub actor: Option<String>,
    /// ci, evaluate, inference, or benchmark.
    pub workflow: Option<String>,
    /// Required for evaluate, inference, and benchmark.
    pub release_version: Option<String>,
    /// Stable retry key. Reusing it with different arguments fails closed.
    pub idempotency_key: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct GetActionLogsParams {
    pub repo: String,
    pub run_id: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct ListProvidersParams {}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct ListReposParams {
    /// Page size. Clotho accepts 1..100 and defaults to 100.
    pub limit: Option<u32>,
    /// Opaque next_cursor returned by the preceding call.
    pub cursor: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct GetActivityParams {
    /// Page size. Clotho accepts 1..100 and defaults to 50.
    pub limit: Option<u32>,
    /// Opaque next_cursor returned by the preceding call.
    pub cursor: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct ListSecretsParams {
    /// org or repo
    pub scope: String,
    /// Org name or repo name depending on scope.
    pub name: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct GetTreeParams {
    pub repo: String,
    pub commit_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct GetFileParams {
    pub repo: String,
    pub path: String,
    pub commit_id: Option<String>,
}

#[derive(Clone)]
pub struct AgentGateway {
    vcs: VcsClient<Channel>,
    diff: DiffClient<Channel>,
    queue: MergeQueueClient<Channel>,
    rest: RestClient,
    identity: IdentityStore,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl AgentGateway {
    pub fn new(
        vcs: Channel,
        diff: Channel,
        queue: Channel,
        rest: RestClient,
        identity: IdentityStore,
    ) -> Self {
        Self {
            vcs: VcsClient::new(vcs),
            diff: DiffClient::new(diff),
            queue: MergeQueueClient::new(queue),
            rest,
            identity,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Create a commit from explicit file contents. The commit is authored under the authenticated agent identity and is not landed on main until submit_change runs."
    )]
    async fn commit(
        &self,
        Parameters(params): Parameters<CommitParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let agent = authed_agent(&ctx)?;
        let default_author_name = agent.name.clone();
        let default_author_email =
            format!("{}@agents.clotho.internal", agent.name.replace(' ', "-"));
        let mut vcs = self.vcs.clone();
        self.run_tool(&ctx, "commit", &repo, args, async move {
            let author_name = params.author_name.unwrap_or(default_author_name);
            let author_email = params.author_email.unwrap_or(default_author_email);
            let response = vcs
                .commit(CommitRequest {
                    repo: params.repo,
                    parent_commit_ids: params.parent_commit_ids.unwrap_or_default(),
                    files: params
                        .files
                        .into_iter()
                        .map(|f| FileChange {
                            path: f.path,
                            content: f.content.into_bytes(),
                            executable: f.executable.unwrap_or(false),
                        })
                        .collect(),
                    deleted_paths: params.deleted_paths.unwrap_or_default(),
                    message: params.message,
                    author_name,
                    author_email,
                })
                .await?
                .into_inner();
            Ok(json!({
                "commit_id": response.commit_id,
                "change_id": response.change_id,
                "operation_id": response.operation_id,
            }))
        })
        .await
    }

    #[tool(
        description = "Submit an agent-authored commit to the merge queue so it lands on main through serialized integration. Conflicts are surfaced in the response instead of blocking."
    )]
    async fn submit_change(
        &self,
        Parameters(params): Parameters<SubmitChangeParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let mut queue = self.queue.clone();
        self.run_tool(&ctx, "submit_change", &repo, args, async move {
            let response = queue
                .submit_change(SubmitChangeRequest {
                    repo: params.repo,
                    commit_id: params.commit_id,
                })
                .await?
                .into_inner();
            Ok(json!({
                "commit_id": response.commit_id,
                "change_id": response.change_id,
                "operation_id": response.operation_id,
                "fast_forwarded": response.fast_forwarded,
                "conflicted": response.conflicted,
                "conflicted_paths": response.conflicted_paths,
            }))
        })
        .await
    }

    #[tool(
        description = "Record a named checkpoint in the repository's operation log and return its operation_id, so this exact state can be restored later with restore_to."
    )]
    async fn checkpoint(
        &self,
        Parameters(params): Parameters<CheckpointParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let mut vcs = self.vcs.clone();
        self.run_tool(&ctx, "checkpoint", &repo, args, async move {
            let response = vcs
                .checkpoint(CheckpointRequest {
                    repo: params.repo,
                    label: params.label,
                })
                .await?
                .into_inner();
            Ok(json!({ "operation_id": response.operation_id }))
        })
        .await
    }

    #[tool(
        description = "Restore the repository to a previous operation (a checkpoint or any op-log entry). Recorded as a new operation — nothing is erased. Returns the new operation_id."
    )]
    async fn restore_to(
        &self,
        Parameters(params): Parameters<RestoreToParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let mut vcs = self.vcs.clone();
        self.run_tool(&ctx, "restore_to", &repo, args, async move {
            let response = vcs
                .restore_to(RestoreToRequest {
                    repo: params.repo,
                    operation_id: params.operation_id,
                })
                .await?
                .into_inner();
            Ok(json!({ "operation_id": response.operation_id }))
        })
        .await
    }

    #[tool(
        description = "Fast situational awareness for a repository: current heads, the main branch target, recent operation log, and a file tree summary."
    )]
    async fn orient_repo(
        &self,
        Parameters(params): Parameters<OrientRepoParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let mut vcs = self.vcs.clone();
        self.run_tool(&ctx, "orient_repo", &repo, args, async move {
            let limit = params.op_log_limit.unwrap_or(20);
            let heads = vcs
                .get_heads(GetHeadsRequest {
                    repo: params.repo.clone(),
                })
                .await?
                .into_inner();
            let op_log = vcs
                .query_op_log(QueryOpLogRequest {
                    repo: params.repo.clone(),
                    limit,
                })
                .await?
                .into_inner();
            let files = vcs
                .list_files(ListFilesRequest {
                    repo: params.repo.clone(),
                    commit_id: String::new(),
                })
                .await?
                .into_inner();
            Ok(json!({
                "repo": params.repo,
                "main_commit_id": heads.main_commit_id,
                "heads": heads.heads.iter().map(|h| json!({
                    "commit_id": h.commit_id,
                    "change_id": h.change_id,
                    "description": h.description,
                    "author_name": h.author_name,
                    "author_email": h.author_email,
                    "timestamp_millis": h.timestamp_millis,
                    "parent_commit_ids": h.parent_commit_ids,
                })).collect::<Vec<_>>(),
                "op_log": op_log.entries.iter().map(|e| json!({
                    "operation_id": e.operation_id,
                    "description": e.description,
                    "end_time_millis": e.end_time_millis,
                })).collect::<Vec<_>>(),
                "files": files.files.iter().map(|f| json!({
                    "path": f.path,
                    "size_bytes": f.size_bytes,
                    "executable": f.executable,
                })).collect::<Vec<_>>(),
            }))
        })
        .await
    }

    #[tool(
        description = "Structured, symbol-level diff between two commits: which functions, structs, classes, and other named symbols were added, modified, or removed — instead of patch text. Defaults to the latest commit on main."
    )]
    async fn diff_symbol(
        &self,
        Parameters(params): Parameters<DiffSymbolParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let mut vcs = self.vcs.clone();
        let mut diff = self.diff.clone();
        self.run_tool(&ctx, "diff_symbol", &repo, args, async move {
            let to_commit_id = match params.to_commit_id {
                Some(id) => id,
                None => {
                    let heads = vcs
                        .get_heads(GetHeadsRequest {
                            repo: params.repo.clone(),
                        })
                        .await?
                        .into_inner();
                    if heads.main_commit_id.is_empty() {
                        return Err(McpError::invalid_params(
                            "repo has no commits on main and no to_commit_id was given",
                            None,
                        )
                        .into());
                    }
                    heads.main_commit_id
                }
            };
            let changes = vcs
                .diff_commits(DiffCommitsRequest {
                    repo: params.repo.clone(),
                    from_commit_id: params.from_commit_id.unwrap_or_default(),
                    to_commit_id,
                })
                .await?
                .into_inner();
            // Paths whose new side is an unresolved jj conflict (ADR-0006):
            // agents get the same first-class conflict signal as the PR view.
            let conflicted: std::collections::HashSet<String> = changes
                .files
                .iter()
                .filter(|f| f.conflicted)
                .map(|f| f.path.clone())
                .collect();
            let structured = diff
                .diff_files(DiffFilesRequest {
                    files: changes
                        .files
                        .into_iter()
                        .map(|f| FileDiffInput {
                            path: f.path,
                            old_content: f.old_content,
                            new_content: f.new_content,
                        })
                        .collect(),
                })
                .await?
                .into_inner();
            Ok(json!({
                "repo": params.repo,
                "from_commit_id": changes.from_commit_id,
                "to_commit_id": changes.to_commit_id,
                "files": structured.files.iter().map(|f| json!({
                    "path": f.path,
                    "language": f.language,
                    "status": status_name(f.status()),
                    "conflicted": conflicted.contains(&f.path),
                    "symbols": f.symbols.iter().map(|s| json!({
                        "name": s.name,
                        "kind": s.kind,
                        "status": status_name(s.status()),
                        "old_start_line": s.old_start_line,
                        "old_end_line": s.old_end_line,
                        "new_start_line": s.new_start_line,
                        "new_end_line": s.new_end_line,
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            }))
        })
        .await
    }

    // -----------------------------------------------------------------------
    // Stage 15: collab + Actions + platform (via REST edge)
    // -----------------------------------------------------------------------

    #[tool(
        description = "List issues on a repository (open|closed|all). Backed by the Clotho REST edge."
    )]
    async fn list_issues(
        &self,
        Parameters(params): Parameters<ListIssuesParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "list_issues", &repo, args, async move {
            let state = params.state.as_deref().unwrap_or("open");
            rest.get(&format!(
                "/api/v1/repos/{}/issues?state={state}",
                urlencoding_path(&params.repo)
            ))
            .await
        })
        .await
    }

    #[tool(description = "Create an issue on a repository. Backed by the Clotho REST edge.")]
    async fn create_issue(
        &self,
        Parameters(params): Parameters<CreateIssueParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "create_issue", &repo, args, async move {
            let mut payload = json!({
                "title": params.title,
                "body": params.body.unwrap_or_default(),
            });
            if let Some(labels) = params.labels {
                if !labels.is_empty() {
                    payload["labels"] = json!(labels);
                }
            }
            if let Some(assignees) = params.assignees {
                if !assignees.is_empty() {
                    payload["assignees"] = json!(assignees);
                }
            }
            if let Some(milestone) = params.milestone {
                payload["milestone"] = json!(milestone);
            }
            rest.post(
                &format!("/api/v1/repos/{}/issues", urlencoding_path(&params.repo)),
                payload,
            )
            .await
        })
        .await
    }

    #[tool(description = "Comment on an issue. Backed by the Clotho REST edge.")]
    async fn comment_issue(
        &self,
        Parameters(params): Parameters<CommentIssueParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "comment_issue", &repo, args, async move {
            rest.post(
                &format!(
                    "/api/v1/repos/{}/issues/{}/comments",
                    urlencoding_path(&params.repo),
                    params.number
                ),
                json!({ "body": params.body }),
            )
            .await
        })
        .await
    }

    #[tool(description = "List pull requests on a repository. Backed by the Clotho REST edge.")]
    async fn list_pulls(
        &self,
        Parameters(params): Parameters<ListPullsParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "list_pulls", &repo, args, async move {
            let state = params.state.as_deref().unwrap_or("all");
            rest.get(&format!(
                "/api/v1/repos/{}/pulls?state={state}",
                urlencoding_path(&params.repo)
            ))
            .await
        })
        .await
    }

    #[tool(description = "Open a pull request. Backed by the Clotho REST edge.")]
    async fn create_pull(
        &self,
        Parameters(params): Parameters<CreatePullParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "create_pull", &repo, args, async move {
            rest.post(
                &format!("/api/v1/repos/{}/pulls", urlencoding_path(&params.repo)),
                json!({
                    "title": params.title,
                    "head": params.head,
                    "base": params.base.unwrap_or_else(|| "main".into()),
                    "body": params.body.unwrap_or_default(),
                }),
            )
            .await
        })
        .await
    }

    #[tool(description = "Comment on a pull request. Backed by the Clotho REST edge.")]
    async fn comment_pull(
        &self,
        Parameters(params): Parameters<CommentPullParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "comment_pull", &repo, args, async move {
            rest.post(
                &format!(
                    "/api/v1/repos/{}/pulls/{}/comments",
                    urlencoding_path(&params.repo),
                    params.number
                ),
                json!({ "body": params.body }),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Submit a PR review (COMMENT, APPROVE, or REQUEST_CHANGES). Backed by the Clotho REST edge."
    )]
    async fn review_pull(
        &self,
        Parameters(params): Parameters<ReviewPullParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "review_pull", &repo, args, async move {
            rest.post(
                &format!(
                    "/api/v1/repos/{}/pulls/{}/reviews",
                    urlencoding_path(&params.repo),
                    params.number
                ),
                json!({
                    "event": params.event.unwrap_or_else(|| "COMMENT".into()),
                    "body": params.body.unwrap_or_default(),
                }),
            )
            .await
        })
        .await
    }

    #[tool(description = "Merge a pull request. Backed by the Clotho REST edge.")]
    async fn merge_pull(
        &self,
        Parameters(params): Parameters<MergePullParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "merge_pull", &repo, args, async move {
            rest.post(
                &format!(
                    "/api/v1/repos/{}/pulls/{}/merge",
                    urlencoding_path(&params.repo),
                    params.number
                ),
                json!({
                    "method": params.method.unwrap_or_else(|| "merge".into()),
                }),
            )
            .await
        })
        .await
    }

    #[tool(description = "List Action runs for a repository. Backed by the Clotho REST edge.")]
    async fn list_action_runs(
        &self,
        Parameters(params): Parameters<ListActionRunsParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "list_action_runs", &repo, args, async move {
            let mut path = format!(
                "/api/v1/repos/{}/actions/runs",
                urlencoding_path(&params.repo)
            );
            if let Some(limit) = params.limit {
                path.push_str(&format!("?limit={limit}"));
            }
            rest.get(&path).await
        })
        .await
    }

    #[tool(
        description = "Start a manual Action run on a repository (uses main head when commit_id is omitted). Backed by the Clotho REST edge."
    )]
    async fn start_action_run(
        &self,
        Parameters(params): Parameters<StartActionRunParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "start_action_run", &repo, args, async move {
            rest.post_with_idempotency_key(
                &format!(
                    "/api/v1/repos/{}/actions/runs",
                    urlencoding_path(&params.repo)
                ),
                json!({
                    "commit_id": params.commit_id.unwrap_or_default(),
                    "branch": params.branch.unwrap_or_else(|| "main".into()),
                    "actor": params.actor.unwrap_or_else(|| "agent".into()),
                    "workflow": params.workflow.unwrap_or_else(|| "ci".into()),
                    "release_version": params.release_version.unwrap_or_default(),
                }),
                params.idempotency_key.as_deref(),
            )
            .await
        })
        .await
    }

    #[tool(description = "Fetch logs for an Action run. Backed by the Clotho REST edge.")]
    async fn get_action_logs(
        &self,
        Parameters(params): Parameters<GetActionLogsParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "get_action_logs", &repo, args, async move {
            rest.get(&format!(
                "/api/v1/repos/{}/actions/runs/{}/logs",
                urlencoding_path(&params.repo),
                urlencoding_path(&params.run_id)
            ))
            .await
        })
        .await
    }

    #[tool(
        description = "List compute providers and their configured/honest status. Platform tool (no repo scope). Backed by the Clotho REST edge."
    )]
    async fn list_providers(
        &self,
        Parameters(_params): Parameters<ListProvidersParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let rest = self.rest.clone();
        self.run_tool(&ctx, "list_providers", "", json!({}), async move {
            rest.get("/api/v1/providers").await
        })
        .await
    }

    #[tool(
        description = "List repositories visible on the Clotho control plane. Platform tool. Backed by the Clotho REST edge."
    )]
    async fn list_repos(
        &self,
        Parameters(params): Parameters<ListReposParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "list_repos", "", args, async move {
            let mut query = Vec::new();
            if let Some(limit) = params.limit {
                query.push(format!("limit={limit}"));
            }
            if let Some(cursor) = &params.cursor {
                query.push(format!("cursor={}", urlencoding_query(cursor)));
            }
            let path = if query.is_empty() {
                "/api/v1/repos".into()
            } else {
                format!("/api/v1/repos?{}", query.join("&"))
            };
            rest.get(&path).await
        })
        .await
    }

    #[tool(
        description = "Read the global activity feed. Platform tool. Backed by the Clotho REST edge."
    )]
    async fn get_activity(
        &self,
        Parameters(params): Parameters<GetActivityParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "get_activity", "", args, async move {
            let mut query = Vec::new();
            if let Some(limit) = params.limit {
                query.push(format!("limit={limit}"));
            }
            if let Some(cursor) = &params.cursor {
                query.push(format!("cursor={}", urlencoding_query(cursor)));
            }
            let path = if query.is_empty() {
                "/api/v1/activity".into()
            } else {
                format!("/api/v1/activity?{}", query.join("&"))
            };
            rest.get(&path).await
        })
        .await
    }

    #[tool(
        description = "List secret metadata (name, last4, description) for an org or repo. Never returns secret values. Scope tool: uses repo name when scope=repo."
    )]
    async fn list_secrets(
        &self,
        Parameters(params): Parameters<ListSecretsParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let scope_repo = if params.scope == "repo" {
            params.name.clone()
        } else {
            String::new()
        };
        let rest = self.rest.clone();
        self.run_tool(&ctx, "list_secrets", &scope_repo, args, async move {
            let path = match params.scope.as_str() {
                "org" => format!("/api/v1/orgs/{}/secrets", urlencoding_path(&params.name)),
                "repo" => format!("/api/v1/repos/{}/secrets", urlencoding_path(&params.name)),
                other => {
                    return Err(McpError::invalid_params(
                        format!("scope must be org or repo, got {other:?}"),
                        None,
                    )
                    .into());
                }
            };
            rest.get(&path).await
        })
        .await
    }

    #[tool(
        description = "Get the file tree for a repository at a commit (default: main head). REST edge."
    )]
    async fn get_tree(
        &self,
        Parameters(params): Parameters<GetTreeParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "get_tree", &repo, args, async move {
            let mut path = format!("/api/v1/repos/{}/tree", urlencoding_path(&params.repo));
            if let Some(cid) = params.commit_id.as_deref() {
                if !cid.is_empty() {
                    path.push_str(&format!("?commit_id={cid}"));
                }
            }
            rest.get(&path).await
        })
        .await
    }

    #[tool(description = "Read a single file from a repository. REST edge.")]
    async fn get_file(
        &self,
        Parameters(params): Parameters<GetFileParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = serde_json::to_value(&params).unwrap_or_default();
        let repo = params.repo.clone();
        let rest = self.rest.clone();
        self.run_tool(&ctx, "get_file", &repo, args, async move {
            let mut path = format!(
                "/api/v1/repos/{}/file?path={}",
                urlencoding_path(&params.repo),
                urlencoding_query(&params.path)
            );
            if let Some(cid) = params.commit_id.as_deref() {
                if !cid.is_empty() {
                    path.push_str(&format!("&commit_id={cid}"));
                }
            }
            rest.get(&path).await
        })
        .await
    }
}

impl AgentGateway {
    /// Shared guard rail for every tool: resolve the authenticated agent from
    /// the request extensions (injected by the HTTP auth middleware), enforce
    /// token scopes, execute, and audit the outcome — success, denial, or
    /// error — before returning.
    async fn run_tool(
        &self,
        ctx: &RequestContext<RoleServer>,
        tool: &'static str,
        repo: &str,
        args: Value,
        work: impl std::future::Future<Output = Result<Value, ToolError>>,
    ) -> Result<CallToolResult, McpError> {
        let agent = authed_agent(ctx)?;
        let bearer = forwarded_agent_bearer(ctx)?;
        let digest = sha256(args.to_string().as_bytes());

        // Empty repo = platform tool (list_providers, get_activity, …): only
        // tool scope is checked. Non-empty repo still requires repo scope.
        let repo_ok = repo.is_empty() || agent.may_touch_repo(repo);
        if !agent.may_use_tool(tool) || !repo_ok {
            let denied = format!(
                "agent {:?} is not authorized for tool {tool:?} on repo {repo:?}",
                agent.name
            );
            self.audit(&agent, tool, repo, &digest, "denied", Some(&denied))
                .await?;
            return Ok(CallToolResult::error(vec![ContentBlock::text(denied)]));
        }

        match with_forwarded_agent_bearer(bearer, work).await {
            Ok(value) => {
                self.audit(&agent, tool, repo, &digest, "ok", None).await?;
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    value.to_string(),
                )]))
            }
            Err(err) => {
                let err = McpError::from(err);
                let message = err.message.to_string();
                self.audit(&agent, tool, repo, &digest, "error", Some(&message))
                    .await?;
                let payload = serde_json::to_value(&err).unwrap_or_else(|_| {
                    json!({
                        "code": -32603,
                        "message": "tool failed and its typed error could not be encoded"
                    })
                });
                Ok(CallToolResult::error(vec![ContentBlock::text(
                    payload.to_string(),
                )]))
            }
        }
    }

    async fn audit(
        &self,
        agent: &AuthedAgent,
        tool: &str,
        repo: &str,
        digest: &[u8],
        status: &str,
        error: Option<&str>,
    ) -> Result<(), McpError> {
        tracing::info!(agent = %agent.name, tool, repo, status, "mcp tool call");
        self.identity
            .record_audit(agent, tool, repo, digest, status, error)
            .await
            .map_err(|e| McpError::internal_error(format!("audit log write failed: {e}"), None))
    }
}

fn authed_agent(ctx: &RequestContext<RoleServer>) -> Result<AuthedAgent, McpError> {
    ctx.extensions
        .get::<http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<AuthedAgent>())
        .cloned()
        .ok_or_else(|| {
            McpError::internal_error(
                "request reached a tool without an authenticated agent",
                None,
            )
        })
}

fn forwarded_agent_bearer(
    ctx: &RequestContext<RoleServer>,
) -> Result<ForwardedAgentBearer, McpError> {
    ctx.extensions
        .get::<http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<ForwardedAgentBearer>())
        .cloned()
        .ok_or_else(|| {
            McpError::internal_error(
                "request reached a tool without an authenticated agent credential",
                None,
            )
        })
}

fn status_name(status: ChangeStatus) -> &'static str {
    match status {
        ChangeStatus::Added => "added",
        ChangeStatus::Modified => "modified",
        ChangeStatus::Removed => "removed",
        ChangeStatus::Unspecified => "unspecified",
    }
}

/// Percent-encode a single path segment (repo names, run ids).
fn urlencoding_path(s: &str) -> String {
    // Minimal encode: keep unreserved; encode everything else.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn urlencoding_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl ServerHandler for AgentGateway {
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.tool_router
            .call(ToolCallContext::new(self, request, context))
            .await
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let agent = authed_agent(&context)?;
        let tools = self
            .tool_router
            .list_all()
            .into_iter()
            .filter(|tool| agent.may_use_tool(tool.name.as_ref()))
            .collect();
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tool_router.get(name).cloned()
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Clotho agent gateway — VCS tools (gRPC) plus collab/Actions/platform tools \
             (REST edge, same contract as humans and the web app).\n\
             VCS: orient_repo, checkpoint, restore_to, diff_symbol, commit, submit_change.\n\
             Collab: list_issues, create_issue, comment_issue, list_pulls, create_pull, \
             comment_pull, review_pull, merge_pull.\n\
             Actions: list_action_runs, start_action_run, get_action_logs.\n\
             Platform: list_providers, list_repos, get_activity, list_secrets (metadata only).\n\
             Read helpers: get_tree, get_file.\n\
             Every tool is scoped by allowed_tools and (when repo-bound) allowed_repos; \
             all invocations are audited.",
        )
    }
}
