//! The MCP tool surface: `checkpoint`, `restore_to`, `diff_symbol`,
//! `orient_repo` — backed by clotho-vcs and clotho-diff over gRPC, guarded
//! by scoped agent tokens, with every invocation audited (docs/prd.md §5
//! Stage 4).

use clotho_common::pb::diff::v1::diff_client::DiffClient;
use clotho_common::pb::diff::v1::{ChangeStatus, DiffFilesRequest, FileDiffInput};
use clotho_common::pb::vcs::v1::vcs_client::VcsClient;
use clotho_common::pb::vcs::v1::{
    CheckpointRequest, DiffCommitsRequest, GetHeadsRequest, ListFilesRequest, QueryOpLogRequest,
    RestoreToRequest,
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, ErrorData as McpError, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_handler, tool_router, RoleServer, ServerHandler};
use serde_json::{json, Value};
use tonic::transport::Channel;

use crate::identity::{sha256, AuthedAgent, IdentityStore};

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

#[derive(Clone)]
pub struct AgentGateway {
    vcs: VcsClient<Channel>,
    diff: DiffClient<Channel>,
    identity: IdentityStore,
}

#[tool_router]
impl AgentGateway {
    pub fn new(vcs: Channel, diff: Channel, identity: IdentityStore) -> Self {
        Self {
            vcs: VcsClient::new(vcs),
            diff: DiffClient::new(diff),
            identity,
        }
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
        let agent = ctx
            .extensions
            .get::<http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<AuthedAgent>())
            .cloned()
            .ok_or_else(|| {
                McpError::internal_error(
                    "request reached a tool without an authenticated agent",
                    None,
                )
            })?;
        let digest = sha256(args.to_string().as_bytes());

        if !agent.may_use_tool(tool) || !agent.may_touch_repo(repo) {
            let denied = format!(
                "agent {:?} is not authorized for tool {tool:?} on repo {repo:?}",
                agent.name
            );
            self.audit(&agent, tool, repo, &digest, "denied", Some(&denied))
                .await?;
            return Ok(CallToolResult::error(vec![ContentBlock::text(denied)]));
        }

        match work.await {
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
                Err(err)
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

fn status_name(status: ChangeStatus) -> &'static str {
    match status {
        ChangeStatus::Added => "added",
        ChangeStatus::Modified => "modified",
        ChangeStatus::Removed => "removed",
        ChangeStatus::Unspecified => "unspecified",
    }
}

#[tool_handler]
impl ServerHandler for AgentGateway {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Clotho agent gateway: version control tools for AI agents. Use orient_repo \
             for situational awareness, checkpoint before risky work, restore_to to roll \
             back, and diff_symbol to see what changed at the symbol level.",
        )
    }
}
