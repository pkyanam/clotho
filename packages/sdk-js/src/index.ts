/**
 * @clotho/sdk-js — typed client for the Clotho API gateway.
 *
 * Hand-written against the gateway's REST surface (crates/clotho-api-gateway);
 * kept deliberately small and dependency-free. Generating this from an
 * OpenAPI spec is a possible future move once the surface stabilizes
 * (docs/adr/0007) — for the prototype the hand-written client is the source
 * of truth the web app compiles against.
 */

// ---------------------------------------------------------------------------
// types mirrored from the gateway's JSON responses
// ---------------------------------------------------------------------------

/** Clotho repository summary from the collaboration facade. */
export interface RepoInfo {
  id: number;
  /** Clotho control-plane id for the repo. */
  clotho_id: string;
  name: string;
  /** Forgejo full name, e.g. `clotho/weave`. */
  full_name: string;
  /** Clotho org name that owns this repo. */
  owner: string;
  html_url: string;
  clone_url: string;
  default_branch: string;
  description: string;
  visibility: string;
  has_issues: boolean;
  has_pull_requests: boolean;
  open_issues_count: number;
  open_pr_counter: number;
  updated_at: string;
  provider: string;
  configured: boolean;
}

/** One jj commit (a real git commit) from clotho-vcs. */
export interface Commit {
  commit_id: string;
  /** jj change id — stable across rewrites. */
  change_id: string;
  description: string;
  author_name: string;
  author_email: string;
  timestamp_millis: number;
  parent_commit_ids: string[];
}

export interface RepoDetail {
  name: string;
  /** Forgejo/clone-path owner. */
  owner: string;
  /** Clotho org that owns this repo. */
  owner_org: string;
  visibility: string;
  default_branch: string;
  clone_url: string;
  provider: string;
  configured: boolean;
  forgejo: RepoInfo;
  /** Commit the `main` bookmark points at; empty while main is unborn. */
  main_commit_id: string;
  /** All current heads — concurrent agents' anonymous heads included. */
  heads: Commit[];
}

export interface CreatedRepo {
  name: string;
  /** Forgejo/clone-path owner. */
  owner: string;
  /** Clotho org that owns this repo. */
  owner_org: string;
  visibility: string;
  default_branch: string;
  clone_url: string;
  provider: string;
  configured: boolean;
  operation_id: string;
  initial_commit_id: string;
  forgejo: RepoInfo;
}

export interface CommitFileInput {
  path: string;
  content: string;
  executable?: boolean;
}

export interface CreatedCommit {
  commit_id: string;
  change_id: string;
  operation_id: string;
}

export interface SubmitChangeResult {
  commit_id: string;
  change_id: string;
  operation_id: string;
  fast_forwarded: boolean;
  conflicted: boolean;
  conflicted_paths: string[];
}

export interface TreeEntry {
  path: string;
  size_bytes: number;
  executable: boolean;
  /** Unresolved jj conflict (first-class, never hidden). */
  conflicted: boolean;
}

export interface Tree {
  commit_id: string;
  files: TreeEntry[];
}

export interface FileContent {
  commit_id: string;
  path: string;
  executable: boolean;
  /** Unresolved jj conflict; content holds its materialized marker text. */
  conflicted: boolean;
  size_bytes: number;
  /** UTF-8 text contents; null when the file is binary. */
  content: string | null;
  binary: boolean;
}

/** One jj operation-log entry — checkpoints, commits, restores, imports. */
export interface OpLogEntry {
  operation_id: string;
  description: string;
  start_time_millis: number;
  end_time_millis: number;
  parent_operation_ids: string[];
}

export interface PullRef {
  ref: string;
  sha: string;
}

export interface IssueLabel {
  name: string;
  color: string;
}

export interface Issue {
  number: number;
  title: string;
  body: string | null;
  state: string;
  user: { login: string };
  labels: IssueLabel[];
  comments: number;
  html_url: string;
  created_at: string;
  updated_at: string;
}

export interface Comment {
  id: number;
  body: string;
  user: { login: string };
  html_url: string;
  created_at: string;
  updated_at: string;
}

export interface IssueDetail {
  issue: Issue;
  comments: Comment[];
}

/** Clotho pull request summary from the collaboration facade. */
export interface Pull {
  number: number;
  title: string;
  body: string | null;
  state: string;
  user: { login: string };
  head: PullRef;
  base: PullRef;
  merge_base: string;
  merged: boolean;
  mergeable: boolean;
  html_url: string;
  created_at: string;
  updated_at: string;
  comments: number;
}

export interface Branch {
  name: string;
  commit: {
    id: string;
    message: string;
    url: string;
  };
  protected: boolean;
}

export interface CommitStatus {
  id: number;
  state: "pending" | "success" | "failure" | "error" | string;
  context: string;
  description: string;
  target_url: string;
  created_at: string;
  updated_at: string;
}

export interface ActionJob {
  id: string;
  run_id: string;
  name: string;
  status: string;
  exit_code: number | null;
}

export interface ActionRun {
  id: string;
  repo: string;
  commit_id: string;
  branch: string;
  status: "queued" | "running" | "success" | "failure" | "error" | "canceled" | string;
  conclusion: string;
  trigger: "push" | "manual" | "agent" | "pull_request" | string;
  actor: string;
  provider: string;
  sandbox_id: string;
  created_at_millis: number;
  started_at_millis: number;
  finished_at_millis: number;
  duration_ms: number;
  jobs: ActionJob[];
}

export interface ActionRunList {
  runs: ActionRun[];
  next_cursor: number | null;
}

export interface ActionLog {
  run_id: string;
  text: string;
}

export interface ActionsConfig {
  enabled: boolean;
  provider: string;
  default_image: string;
  timeout_seconds: number;
}

export interface ComputeProvider {
  id: string;
  name: string;
  enabled: boolean;
  configured: boolean;
  capabilities: string[];
}

export type DiffLineKind = "context" | "add" | "del";

export interface DiffLine {
  kind: DiffLineKind;
  old_line: number | null;
  new_line: number | null;
  text: string;
}

export interface DiffHunk {
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  lines: DiffLine[];
}

export type ChangeStatus = "added" | "modified" | "removed" | "deleted";

/** Symbol-level change from clotho-diff (tree-sitter). */
export interface SymbolChange {
  name: string;
  kind: string;
  status: ChangeStatus;
  old_start_line: number;
  old_end_line: number;
  new_start_line: number;
  new_end_line: number;
}

export interface FileDiff {
  path: string;
  status: ChangeStatus;
  /** Detected language ("rust", "typescript"); empty when unsupported. */
  language: string;
  /** New side is an unresolved jj conflict; hunks contain its markers. */
  conflicted: boolean;
  binary: boolean;
  symbols: SymbolChange[];
  hunks: DiffHunk[];
}

/**
 * The structured PR diff: the same object that feeds the agent-facing
 * diff_symbol MCP tool, rendered for humans (docs/prd.md §2).
 */
export interface PullDiff {
  from_commit_id: string;
  to_commit_id: string;
  conflicted: boolean;
  files: FileDiff[];
}

/**
 * One agent session's recent activity on a repo — an (agent, token) pair
 * aggregated from the agent gateway's audit log.
 */
export interface AgentSession {
  agent: string;
  agent_id: string;
  token_id: string;
  last_tool: string;
  last_status: "ok" | "denied" | "error";
  last_seen: string;
  first_seen: string;
  tool_calls: number;
}

export interface User {
  id: string;
  name: string;
  email: string;
  display_name: string;
  created_at: string;
}

export interface Org {
  id: string;
  name: string;
  display_name: string;
  forgejo_owner: string;
  created_by: string;
  created_at: string;
}

export interface OrgMembership {
  org_id: string;
  user_id: string;
  role: "admin" | "member" | string;
  user_name: string;
  user_display_name: string;
}

export interface OrgDetail {
  org: Org;
  members: OrgMembership[];
}

export interface ActivityEvent {
  id: number;
  actor_id: string;
  org_id: string | null;
  repo_id: string | null;
  event_type: string;
  payload: unknown;
  created_at: string;
}

export interface HealthStatus {
  service: string;
  version: string;
  status: string;
}

// ---------------------------------------------------------------------------
// client
// ---------------------------------------------------------------------------

export interface ClothoClientOptions {
  /** Base URL of the Clotho API gateway, e.g. http://localhost:8080 */
  baseUrl: string;
  fetch?: typeof fetch;
}

export class ClothoApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = "ClothoApiError";
  }
}

export class ClothoClient {
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;

  constructor(options: ClothoClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
    this.fetchImpl = options.fetch ?? fetch;
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await this.fetchImpl(`${this.baseUrl}${path}`, init);
    if (!res.ok) {
      let message = `${init?.method ?? "GET"} ${path} failed: ${res.status}`;
      try {
        const body = (await res.json()) as { error?: string };
        if (body.error) message = body.error;
      } catch {
        // non-JSON error body; keep the generic message
      }
      throw new ClothoApiError(res.status, message);
    }
    return (await res.json()) as T;
  }

  health(): Promise<HealthStatus> {
    return this.request("/healthz");
  }

  /** Provision a repo in clotho-vcs and Forgejo in one call (ADR-0003). */
  createRepo(
    name: string,
    options?: {
      description?: string;
      visibility?: "public" | "private" | "internal" | string;
      defaultBranch?: string;
      ownerOrg?: string;
    },
  ): Promise<CreatedRepo> {
    return this.request("/api/v1/repos", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        name,
        description: options?.description,
        visibility: options?.visibility ?? "public",
        default_branch: options?.defaultBranch ?? "main",
        owner_org: options?.ownerOrg,
      }),
    });
  }

  async listRepos(): Promise<RepoInfo[]> {
    const { repos } = await this.request<{ repos: RepoInfo[] }>(
      "/api/v1/repos",
    );
    return repos;
  }

  getRepo(name: string): Promise<RepoDetail> {
    return this.request(`/api/v1/repos/${encodeURIComponent(name)}`);
  }

  createCommit(
    name: string,
    options: {
      message: string;
      files: CommitFileInput[];
      deletedPaths?: string[];
      parentCommitIds?: string[];
      authorName?: string;
      authorEmail?: string;
    },
  ): Promise<CreatedCommit> {
    return this.request(`/api/v1/repos/${encodeURIComponent(name)}/commits`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        message: options.message,
        files: options.files,
        deleted_paths: options.deletedPaths ?? [],
        parent_commit_ids: options.parentCommitIds ?? [],
        author_name: options.authorName,
        author_email: options.authorEmail,
      }),
    });
  }

  submitChange(name: string, commitId: string): Promise<SubmitChangeResult> {
    return this.request(`/api/v1/repos/${encodeURIComponent(name)}/submit`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ commit_id: commitId }),
    });
  }

  tree(name: string, commitId?: string): Promise<Tree> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/tree${qs({ commit_id: commitId })}`,
    );
  }

  file(name: string, path: string, commitId?: string): Promise<FileContent> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/file${qs({ path, commit_id: commitId })}`,
    );
  }

  async commits(
    name: string,
    options?: { fromCommitId?: string; limit?: number },
  ): Promise<Commit[]> {
    const { commits } = await this.request<{ commits: Commit[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/commits${qs({
        from_commit_id: options?.fromCommitId,
        limit: options?.limit,
      })}`,
    );
    return commits;
  }

  async opLog(name: string, limit?: number): Promise<OpLogEntry[]> {
    const { entries } = await this.request<{ entries: OpLogEntry[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/oplog${qs({ limit })}`,
    );
    return entries;
  }

  async pulls(
    name: string,
    state: "open" | "closed" | "all" = "all",
  ): Promise<Pull[]> {
    const { pulls } = await this.request<{ pulls: Pull[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/pulls${qs({ state })}`,
    );
    return pulls;
  }

  createPull(
    name: string,
    options: { title: string; body?: string; head: string; base?: string },
  ): Promise<Pull> {
    return this.request(`/api/v1/repos/${encodeURIComponent(name)}/pulls`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        title: options.title,
        body: options.body ?? "",
        head: options.head,
        base: options.base ?? "main",
      }),
    });
  }

  pull(name: string, number: number): Promise<Pull> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/pulls/${number}`,
    );
  }

  pullDiff(name: string, number: number): Promise<PullDiff> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/pulls/${number}/diff`,
    );
  }

  commentOnPull(name: string, number: number, body: string): Promise<Comment> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/pulls/${number}/comments`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ body }),
      },
    );
  }

  reviewPull(
    name: string,
    number: number,
    options: { body?: string; event?: "COMMENT" | "APPROVE" | "REQUEST_CHANGES" },
  ): Promise<Comment> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/pulls/${number}/reviews`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          body: options.body ?? "",
          event: options.event ?? "COMMENT",
        }),
      },
    );
  }

  mergePull(
    name: string,
    number: number,
    options?: { method?: "merge" | "rebase" | "rebase-merge" | "squash"; title?: string; message?: string },
  ): Promise<Pull> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/pulls/${number}/merge`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          method: options?.method ?? "merge",
          title: options?.title,
          message: options?.message,
        }),
      },
    );
  }

  async issues(
    name: string,
    state: "open" | "closed" | "all" = "open",
  ): Promise<Issue[]> {
    const { issues } = await this.request<{ issues: Issue[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/issues${qs({ state })}`,
    );
    return issues;
  }

  createIssue(
    name: string,
    options: { title: string; body?: string },
  ): Promise<Issue> {
    return this.request(`/api/v1/repos/${encodeURIComponent(name)}/issues`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ title: options.title, body: options.body ?? "" }),
    });
  }

  issue(name: string, number: number): Promise<IssueDetail> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/issues/${number}`,
    );
  }

  commentOnIssue(
    name: string,
    number: number,
    body: string,
  ): Promise<Comment> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/issues/${number}/comments`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ body }),
      },
    );
  }

  async branches(name: string): Promise<Branch[]> {
    const { branches } = await this.request<{ branches: Branch[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/branches`,
    );
    return branches;
  }

  async commitStatuses(name: string, sha: string): Promise<CommitStatus[]> {
    const { statuses } = await this.request<{ statuses: CommitStatus[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/commits/${encodeURIComponent(
        sha,
      )}/statuses`,
    );
    return statuses;
  }

  actionRunsPage(
    name: string,
    options?: { limit?: number; before?: number },
  ): Promise<ActionRunList> {
    return this.request<ActionRunList>(
      `/api/v1/repos/${encodeURIComponent(name)}/actions/runs${qs({
        limit: options?.limit,
        before: options?.before,
      })}`,
    );
  }

  async actionRuns(
    name: string,
    options?: { limit?: number; before?: number },
  ): Promise<ActionRun[]> {
    const { runs } = await this.actionRunsPage(name, options);
    return runs;
  }

  createActionRun(
    name: string,
    options?: { commitId?: string; branch?: string; actor?: string },
  ): Promise<ActionRun> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/actions/runs`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          commit_id: options?.commitId ?? "",
          branch: options?.branch ?? "main",
          actor: options?.actor ?? "manual",
        }),
      },
    );
  }

  actionRun(name: string, runId: string): Promise<ActionRun> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/actions/runs/${encodeURIComponent(
        runId,
      )}`,
    );
  }

  actionLogs(name: string, runId: string): Promise<ActionLog> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/actions/runs/${encodeURIComponent(
        runId,
      )}/logs`,
    );
  }

  actionsConfig(name: string): Promise<ActionsConfig> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/actions/config`,
    );
  }

  updateActionsConfig(
    name: string,
    config: Partial<ActionsConfig>,
  ): Promise<ActionsConfig> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/actions/config`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(config),
      },
    );
  }

  async computeProviders(): Promise<ComputeProvider[]> {
    const { providers } = await this.request<{ providers: ComputeProvider[] }>(
      "/api/v1/compute/providers",
    );
    return providers;
  }

  computeProvider(provider: string): Promise<ComputeProvider> {
    return this.request(
      `/api/v1/compute/providers/${encodeURIComponent(provider)}`,
    );
  }

  // Stage 11: users, orgs, activity, and org-scoped repos.
  async users(): Promise<User[]> {
    const { users } = await this.request<{ users: User[] }>("/api/v1/users");
    return users;
  }

  async orgs(): Promise<Org[]> {
    const { orgs } = await this.request<{ orgs: Org[] }>("/api/v1/orgs");
    return orgs;
  }

  createOrg(
    name: string,
    options?: { displayName?: string; forgejoOwner?: string },
  ): Promise<Org> {
    return this.request("/api/v1/orgs", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        name,
        display_name: options?.displayName,
        forgejo_owner: options?.forgejoOwner,
      }),
    });
  }

  getOrg(org: string): Promise<OrgDetail> {
    return this.request(`/api/v1/orgs/${encodeURIComponent(org)}`);
  }

  async getOrgRepos(org: string): Promise<RepoInfo[]> {
    const { repos } = await this.request<{ repos: RepoInfo[] }>(
      `/api/v1/orgs/${encodeURIComponent(org)}/repos`,
    );
    return repos;
  }

  async activity(options?: { limit?: number }): Promise<ActivityEvent[]> {
    const { events } = await this.request<{ events: ActivityEvent[] }>(
      `/api/v1/activity${qs({ limit: options?.limit })}`,
    );
    return events;
  }

  /** Recent agent sessions on a repo (poll this for the presence panel). */
  async agentSessions(
    name: string,
    options?: { limit?: number; withinSecs?: number },
  ): Promise<AgentSession[]> {
    const { sessions } = await this.request<{ sessions: AgentSession[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/agent-sessions${qs({
        limit: options?.limit,
        within_secs: options?.withinSecs,
      })}`,
    );
    return sessions;
  }
}

/** Build a query string, skipping empty/undefined values. */
function qs(params: Record<string, string | number | undefined>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== "") search.set(key, String(value));
  }
  const encoded = search.toString();
  return encoded ? `?${encoded}` : "";
}
