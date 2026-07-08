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

/** Forgejo project entry, as proxied by the gateway. */
export interface RepoInfo {
  id: number;
  name: string;
  full_name: string;
  html_url: string;
  default_branch: string;
  description: string;
  has_issues: boolean;
  has_pull_requests: boolean;
  open_issues_count: number;
  open_pr_counter: number;
  updated_at: string;
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
  owner: string;
  forgejo: RepoInfo;
  /** Commit the `main` bookmark points at; empty while main is unborn. */
  main_commit_id: string;
  /** All current heads — concurrent agents' anonymous heads included. */
  heads: Commit[];
}

export interface CreatedRepo {
  name: string;
  owner: string;
  operation_id: string;
  initial_commit_id: string;
  forgejo: RepoInfo;
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

/** Forgejo pull request, as proxied by the gateway. */
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
  createRepo(name: string): Promise<CreatedRepo> {
    return this.request("/api/v1/repos", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name }),
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
