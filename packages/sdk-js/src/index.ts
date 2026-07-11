/**
 * @clotho/sdk-js — typed client for the Clotho API gateway.
 *
 * Hand-written against the gateway's REST surface (crates/clotho-api-gateway)
 * and kept aligned with docs/openapi.yaml (Stage 15 product contract). CI
 * checks OpenAPI path drift via clotho-api-gateway tests/openapi_drift.rs.
 * Generating the SDK from OpenAPI remains a future hardening option
 * (docs/adr/0007); for now the hand-written client is what the web app compiles
 * against.
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
  /** Full name, e.g. `clotho/weave`. */
  full_name: string;
  /** Clotho org name that owns this repo. */
  owner: string;
  html_url: string;
  clone_url: string;
  default_branch: string;
  description: string;
  visibility: string;
  kind: "code" | "model" | "dataset" | string;
  large_file_threshold_bytes: number;
  network_mode: "public" | "tailscale" | string;
  network_tags: string[];
  has_issues: boolean;
  has_pull_requests: boolean;
  open_issues_count: number;
  open_pr_counter: number;
  updated_at: string;
  provider: string;
  configured: boolean;
}

export interface RepoList {
  repos: RepoInfo[];
  next_cursor: string | null;
}

export interface RepoPageOptions {
  /** Page size. REST accepts 1..100; defaults to 100. */
  limit?: number;
  /** Opaque cursor returned by the preceding page. */
  cursor?: string;
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
  /** Git clone-path owner. */
  owner: string;
  /** Clotho org that owns this repo. */
  owner_org: string;
  description: string;
  visibility: string;
  kind: "code" | "model" | "dataset" | string;
  large_file_threshold_bytes: number;
  network_mode: "public" | "tailscale" | string;
  network_tags: string[];
  default_branch: string;
  clone_url: string;
  provider: string;
  configured: boolean;
  info: RepoInfo;
  /** Commit the `main` bookmark points at; empty while main is unborn. */
  main_commit_id: string;
  /** All current heads — concurrent agents' anonymous heads included. */
  heads: Commit[];
}

export interface CreatedRepo {
  name: string;
  /** Git clone-path owner. */
  owner: string;
  /** Clotho org that owns this repo. */
  owner_org: string;
  description: string;
  visibility: string;
  kind: "code" | "model" | "dataset" | string;
  large_file_threshold_bytes: number;
  network_mode: "public" | "tailscale" | string;
  network_tags: string[];
  default_branch: string;
  clone_url: string;
  provider: string;
  configured: boolean;
  operation_id: string;
  initial_commit_id: string;
  info: RepoInfo;
}

export interface CommitFileInput {
  path: string;
  /** UTF-8 payload. Use content_base64 instead for binary artifacts. */
  content?: string;
  /** Standard base64 payload; mutually exclusive with content. */
  content_base64?: string;
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

export interface ArtifactEntry {
  path: string;
  role: string;
  format: string;
  family: string;
  /** Logical bytes after Arachne pointer composition. */
  size_bytes: number;
  storage: "git" | "arachne" | string;
  oid_sha256: string;
  arachne_hash: string;
  conflicted: boolean;
}

export interface ArtifactManifest {
  commit_id: string;
  kind: "code" | "model" | "dataset" | string;
  total_files: number;
  total_bytes: number;
  arachne_files: number;
  role_counts: Record<string, number>;
  format_counts: Record<string, number>;
  /** Card frontmatter plus bounded model/dataset config metadata. */
  metadata: Record<string, unknown>;
  metadata_sources: string[];
  readiness: {
    card: boolean;
    primary_artifacts: boolean;
    metadata: boolean;
    ready: boolean;
    warnings: string[];
  };
  artifacts: ArtifactEntry[];
}

export interface ArtifactPreview {
  commit_id: string;
  path: string;
  format: "csv" | "tsv" | "jsonl" | string;
  columns: string[];
  rows: unknown[][];
  bytes_read: number;
  truncated: boolean;
}

export interface HubImportResult {
  provider: string;
  source_repo_id: string;
  source_revision: string;
  commit_id: string;
  operation_id: string;
  files_imported: number;
  logical_bytes: number;
  arachne_files: number;
  security_counts: Record<string, number>;
  fast_forwarded: boolean;
  conflicted: boolean;
  conflicted_paths: string[];
}

export interface HubImportJob {
  id: string;
  repo_id: string;
  provider: string;
  source_repo_id: string;
  source_revision: string;
  status: "queued" | "running" | "succeeded" | "failed" | "interrupted" | string;
  files_total: number;
  files_imported: number;
  logical_bytes: number;
  bytes_imported: number;
  arachne_files: number;
  security_counts: Record<string, number>;
  commit_id: string;
  operation_id: string;
  error: string;
  created_by: string;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
}

export interface RepoReleaseSummary {
  id: string;
  version: string;
  commit_id: string;
  manifest_sha256: string;
  kind: string;
  total_files: number;
  total_bytes: number;
  ready: boolean;
  verified: boolean;
  created_by: string;
  created_at: string;
}

export interface RepoRelease extends RepoReleaseSummary {
  manifest: ArtifactManifest;
}

export interface HubCatalogEntry {
  id: string;
  modelId: string;
  author: string;
  sha: string;
  lastModified: string;
  private: boolean;
  tags: string[];
  pipeline_tag?: string | null;
  library_name?: string | null;
  usedStorage: number;
  clotho: {
    release: string;
    manifest_sha256: string;
    evaluation_count?: number;
    source_of_truth: true;
  };
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
  /** Standard base64 bytes when binary is true. */
  content_base64?: string;
  binary: boolean;
}

export interface ArachneFile {
  path: string;
  logical_bytes: number;
  pointer_bytes: number;
  oid_sha256: string;
  arachne_hash: string;
}

export interface RepoStorageStats {
  commit_id: string;
  git_tree_bytes: number;
  logical_bytes: number;
  arachne_file_count: number;
  arachne_logical_bytes: number;
  large_files: ArachneFile[];
  store_scope: string;
  xorb_count: number;
  xorb_bytes: number;
  shard_count: number;
  shard_bytes: number;
  store_total_bytes: number;
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

export interface Label {
  id: number;
  name: string;
  color: string;
  description: string;
}

export interface Milestone {
  id: number;
  title: string;
  state: string;
  description: string;
  due_on: string | null;
}

export interface Issue {
  number: number;
  title: string;
  body: string | null;
  state: string;
  user: { login: string };
  labels: IssueLabel[];
  assignees: { login: string }[];
  milestone: Milestone | null;
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
  /** Parent comment id when upstream supports threaded replies. */
  in_reply_to?: number | null;
  pull_request_review_id?: number | null;
}

export interface Review {
  id: number;
  body: string;
  user: { login: string };
  state: string;
  html_url: string;
  submitted_at: string;
}

export interface MergePolicy {
  require_passing_actions: boolean;
  block_merge_when_conflicted: boolean;
  require_review_approvals: number;
  protect_default_branch: boolean;
  updated_at: string;
}

export interface UpdateMergePolicyRequest {
  require_passing_actions?: boolean;
  block_merge_when_conflicted?: boolean;
  require_review_approvals?: number;
  protect_default_branch?: boolean;
}

export interface IssueDetail {
  issue: Issue;
  comments: Comment[];
}

export interface Notification {
  id: number;
  user_id: string;
  repo_name: string | null;
  kind: string;
  title: string;
  body: string;
  href: string;
  read_at: string | null;
  created_at: string;
}

export interface NotificationList {
  notifications: Notification[];
  unread_count: number;
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
  workflow: "ci" | "evaluate" | "inference" | "benchmark" | string;
  release_version: string;
  release_manifest_sha256: string;
  provider: string;
  sandbox_id: string;
  created_at_millis: number;
  started_at_millis: number;
  finished_at_millis: number;
  duration_ms: number;
  attempt: number;
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
  accelerator: "cpu" | "gpu" | string;
  gpu_types: string[];
}

/** Structured capability flags from the CCI provider registry (Stage 12). */
export interface ProviderCapabilities {
  one_shot_jobs: boolean;
  persistent_workspaces: boolean;
  snapshots: boolean;
  templates: boolean;
  regions: string[];
  ssh: boolean;
  desktop: boolean;
  public_url: boolean;
  file_api: boolean;
  terminal_streaming: boolean;
  gpu: boolean;
  gpu_types: string[];
  cost_hints: string;
}

export interface ComputeProvider {
  id: string;
  name: string;
  /** Implementation kind: direct | bridge | stub. */
  kind?: string;
  enabled: boolean;
  configured: boolean;
  configured_reason?: string;
  /** Flat capability tags for badges. */
  capabilities: string[];
  /** Structured capabilities (Stage 12). */
  capability_detail?: ProviderCapabilities;
  default_snapshot?: string;
  notes?: string;
}

export interface ComputeProviderList {
  providers: ComputeProvider[];
  default_provider_id: string;
}

/** Provider Fabric layer (ADR-0019 / Stage 17). */
export type ProviderLayer = "compute" | "storage" | "network" | "hub" | "auth";

export interface FabricProvider {
  id: string;
  name: string;
  layer: ProviderLayer | string;
  kind: string;
  enabled: boolean;
  configured: boolean;
  configured_reason?: string;
  capabilities: string[];
  notes?: string;
}

export interface FabricProviderList {
  providers: FabricProvider[];
  default_provider_id: string;
  layer?: string | null;
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

/** Agent identity (non-human principal). */
export interface Agent {
  id: string;
  name: string;
  description: string;
  created_at: string;
  disabled_at?: string | null;
}

/** Agent token metadata — never includes plaintext secrets. */
export interface AgentTokenMeta {
  id: string;
  token_prefix: string;
  allowed_repos: string[];
  allowed_tools: string[];
  created_at: string;
  expires_at: string | null;
  revoked_at: string | null;
}

/** Minted agent token — plaintext shown once at creation. */
export interface MintedAgentToken {
  token: string;
  token_id: string;
  agent: string;
  allowed_repos: string[];
  allowed_tools: string[];
  expires_at: string | null;
}

export interface AgentDetail {
  agent: Agent;
  tokens: AgentTokenMeta[];
}

export interface AgentAuditEntry {
  id: number;
  agent_id: string;
  token_id: string;
  tool: string;
  repo: string;
  args_digest: string;
  status: string;
  error: string | null;
  occurred_at: string;
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
  created_by: string;
  created_at: string;
}

export interface UserMe {
  id: string;
  name: string;
  email: string;
  display_name: string;
  created_at: string;
}

export interface MeResponse {
  user: UserMe;
  token_id: string | null;
}

export interface ApiTokenMeta {
  id: string;
  name: string;
  token_prefix: string;
  scopes: string[];
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
}

export interface CreatedApiToken extends ApiTokenMeta {
  /** Plaintext token — shown once at creation. */
  token: string;
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

export interface ActivityPage {
  events: ActivityEvent[];
  next_cursor: string | null;
}

export interface ActivityPageOptions {
  limit?: number;
  cursor?: string;
}

export interface HealthStatus {
  service: string;
  version: string;
  status: string;
}

/** Metadata-only secret view — never includes plaintext (docs/adr/0014). */
export interface SecretMeta {
  id: string;
  scope: "org" | "repo" | string;
  org_id: string | null;
  repo_id: string | null;
  name: string;
  description: string;
  /** Last 4 characters of the secret value for UI masking. */
  value_last4: string;
  created_by: string;
  created_at: string;
  updated_at: string;
}

// ---------------------------------------------------------------------------
// client
// ---------------------------------------------------------------------------

export interface ClothoClientOptions {
  /** Base URL of the Clotho API gateway, e.g. http://localhost:8080 */
  baseUrl: string;
  /** Bearer token for authenticated requests (CLOTHO_TOKEN). */
  token?: string;
  fetch?: typeof fetch;
}

export interface ErrorEnvelope {
  version: "1";
  code: string;
  message: string;
  request_id: string;
  retryable: boolean;
  details?: unknown;
}

export class ClothoApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
    public readonly code = "http_error",
    public readonly requestId = "",
    public readonly retryable = false,
    public readonly details?: unknown,
  ) {
    super(message);
    this.name = "ClothoApiError";
  }
}

async function responseError(
  response: Response,
  fallback: string,
): Promise<ClothoApiError> {
  try {
    const body = (await response.json()) as Partial<ErrorEnvelope> & {
      error?: string;
    };
    const message = body.message ?? body.error ?? fallback;
    return new ClothoApiError(
      response.status,
      message,
      body.code ?? "http_error",
      body.request_id ?? response.headers.get("x-request-id") ?? "",
      body.retryable ?? false,
      body.details,
    );
  } catch {
    return new ClothoApiError(
      response.status,
      fallback,
      "http_error",
      response.headers.get("x-request-id") ?? "",
    );
  }
}

export class ClothoClient {
  private readonly baseUrl: string;
  private readonly token?: string;
  private readonly fetchImpl: typeof fetch;

  constructor(options: ClothoClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
    this.token = options.token;
    this.fetchImpl = options.fetch ?? fetch;
  }

  private headers(init?: RequestInit): Record<string, string> {
    const h: Record<string, string> = {};
    if (this.token) {
      h.authorization = `Bearer ${this.token}`;
    }
    const extra = init?.headers;
    if (extra instanceof Headers) {
      extra.forEach((v, k) => {
        h[k] = v;
      });
    } else if (Array.isArray(extra)) {
      for (const [k, v] of extra) h[k] = v;
    } else if (extra && typeof extra === "object") {
      Object.assign(h, extra);
    }
    return h;
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await this.fetchImpl(`${this.baseUrl}${path}`, {
      ...init,
      headers: this.headers(init),
    });
    if (!res.ok) {
      throw await responseError(
        res,
        `${init?.method ?? "GET"} ${path} failed: ${res.status}`,
      );
    }
    if (res.status === 204 || res.headers.get("content-length") === "0") {
      return undefined as T;
    }
    const text = await res.text();
    if (!text) return undefined as T;
    return JSON.parse(text) as T;
  }

  health(): Promise<HealthStatus> {
    return this.request("/healthz");
  }

  /** Provision a repo in clotho-vcs and the collaboration facade in one call. */
  createRepo(
    name: string,
    options?: {
      description?: string;
      visibility?: "public" | "private" | "internal" | string;
      defaultBranch?: string;
      kind?: "code" | "model" | "dataset";
      largeFileThresholdBytes?: number;
      networkMode?: "public" | "tailscale";
      networkTags?: string[];
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
        kind: options?.kind ?? "code",
        large_file_threshold_bytes: options?.largeFileThresholdBytes,
        network_mode: options?.networkMode ?? "public",
        network_tags: options?.networkTags ?? [],
      }),
    });
  }

  listReposPage(options?: RepoPageOptions): Promise<RepoList> {
    return this.request<RepoList>(
      `/api/v1/repos${qs({
        limit: options?.limit,
        cursor: options?.cursor,
      })}`,
    );
  }

  /** Compatibility helper that follows bounded REST pages, up to 10,000 repos. */
  async listRepos(): Promise<RepoInfo[]> {
    return this.collectRepoPages((options) => this.listReposPage(options));
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

  artifacts(name: string, commitId?: string): Promise<ArtifactManifest> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/artifacts${qs({ commit_id: commitId })}`,
    );
  }

  artifactPreview(
    name: string,
    path: string,
    options?: { commitId?: string; limit?: number },
  ): Promise<ArtifactPreview> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/artifacts/preview${qs({
        path,
        commit_id: options?.commitId,
        limit: options?.limit,
      })}`,
    );
  }

  importHuggingFace(
    name: string,
    repoId: string,
    options?: {
      revision?: string;
      paths?: string[];
      maxFiles?: number;
      maxTotalBytes?: number;
      allowUnsafe?: boolean;
    },
  ): Promise<HubImportResult> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/imports/huggingface`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          repo_id: repoId,
          revision: options?.revision ?? "main",
          paths: options?.paths ?? [],
          max_files: options?.maxFiles ?? 200,
          max_total_bytes: options?.maxTotalBytes ?? 50 * 1024 * 1024 * 1024,
          allow_unsafe: options?.allowUnsafe ?? false,
        }),
      },
    );
  }

  startHuggingFaceImport(
    name: string,
    repoId: string,
    options?: {
      revision?: string;
      paths?: string[];
      maxFiles?: number;
      maxTotalBytes?: number;
      allowUnsafe?: boolean;
    },
  ): Promise<HubImportJob> {
    return this.request(`/api/v1/repos/${encodeURIComponent(name)}/hub-imports`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        repo_id: repoId,
        revision: options?.revision ?? "main",
        paths: options?.paths ?? [],
        max_files: options?.maxFiles ?? 200,
        max_total_bytes: options?.maxTotalBytes ?? 50 * 1024 * 1024 * 1024,
        allow_unsafe: options?.allowUnsafe ?? false,
      }),
    });
  }

  async hubImportJobs(name: string): Promise<HubImportJob[]> {
    const response = await this.request<{ jobs: HubImportJob[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/hub-imports`,
    );
    return response.jobs;
  }

  hubImportJob(name: string, id: string): Promise<HubImportJob> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/hub-imports/${encodeURIComponent(id)}`,
    );
  }

  async releases(name: string): Promise<RepoReleaseSummary[]> {
    const response = await this.request<{ releases: RepoReleaseSummary[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/releases`,
    );
    return response.releases;
  }

  hubModels(options?: {
    search?: string;
    filter?: string;
    author?: string;
    pipelineTag?: string;
    limit?: number;
  }): Promise<HubCatalogEntry[]> {
    return this.request(
      `/api/models${qs({
        search: options?.search,
        filter: options?.filter,
        author: options?.author,
        pipeline_tag: options?.pipelineTag,
        limit: options?.limit,
        full: "true",
      })}`,
    );
  }

  hubDatasets(options?: {
    search?: string;
    filter?: string;
    author?: string;
    limit?: number;
  }): Promise<HubCatalogEntry[]> {
    return this.request(
      `/api/datasets${qs({
        search: options?.search,
        filter: options?.filter,
        author: options?.author,
        limit: options?.limit,
        full: "true",
      })}`,
    );
  }

  release(name: string, version: string): Promise<RepoRelease> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/releases/${encodeURIComponent(version)}`,
    );
  }

  createRelease(
    name: string,
    version: string,
    options?: { commitId?: string; requireReady?: boolean },
  ): Promise<RepoRelease> {
    return this.request(`/api/v1/repos/${encodeURIComponent(name)}/releases`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        version,
        commit_id: options?.commitId ?? "",
        require_ready: options?.requireReady ?? true,
      }),
    });
  }

  async downloadReleaseFile(
    name: string,
    version: string,
    path: string,
    options?: { head?: boolean; range?: { start: number; end?: number } },
  ): Promise<Response> {
    const encodedPath = path.split("/").map(encodeURIComponent).join("/");
    const endpoint = `/api/v1/repos/${encodeURIComponent(name)}/releases/${encodeURIComponent(version)}/resolve/${encodedPath}`;
    const headers = this.headers();
    if (options?.range) {
      const { start, end } = options.range;
      if (
        !Number.isSafeInteger(start) ||
        start < 0 ||
        (end != null && (!Number.isSafeInteger(end) || end < start))
      ) {
        throw new RangeError("release byte range must be non-negative and ordered");
      }
      headers.range = `bytes=${options.range.start}-${options.range.end ?? ""}`;
    }
    const response = await this.fetchImpl(`${this.baseUrl}${endpoint}`, {
      method: options?.head ? "HEAD" : "GET",
      headers,
    });
    if (!response.ok) {
      throw await responseError(
        response,
        `${options?.head ? "HEAD" : "GET"} ${endpoint} failed: ${response.status}`,
      );
    }
    return response;
  }

  file(name: string, path: string, commitId?: string): Promise<FileContent> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/file${qs({ path, commit_id: commitId })}`,
    );
  }

  storageStats(name: string): Promise<RepoStorageStats> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/storage`,
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

  commentOnPull(
    name: string,
    number: number,
    body: string,
    options?: { in_reply_to?: number },
  ): Promise<Comment> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/pulls/${number}/comments`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          body,
          ...(options?.in_reply_to != null
            ? { in_reply_to: options.in_reply_to }
            : {}),
        }),
      },
    );
  }

  async listPullComments(name: string, number: number): Promise<Comment[]> {
    const { comments } = await this.request<{ comments: Comment[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/pulls/${number}/comments`,
    );
    return comments;
  }

  async listPullReviews(name: string, number: number): Promise<Review[]> {
    const { reviews } = await this.request<{ reviews: Review[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/pulls/${number}/reviews`,
    );
    return reviews;
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
    filters?: {
      labels?: string;
      assignee?: string;
      milestone?: number;
    },
  ): Promise<Issue[]> {
    const { issues } = await this.request<{ issues: Issue[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/issues${qs({
        state,
        labels: filters?.labels,
        assignee: filters?.assignee,
        milestone: filters?.milestone,
      })}`,
    );
    return issues;
  }

  createIssue(
    name: string,
    options: {
      title: string;
      body?: string;
      labels?: string[];
      assignees?: string[];
      milestone?: number;
    },
  ): Promise<Issue> {
    return this.request(`/api/v1/repos/${encodeURIComponent(name)}/issues`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        title: options.title,
        body: options.body ?? "",
        labels: options.labels ?? [],
        assignees: options.assignees ?? [],
        milestone: options.milestone,
      }),
    });
  }

  updateIssue(
    name: string,
    number: number,
    options: {
      title?: string;
      body?: string;
      state?: "open" | "closed";
      labels?: string[];
      assignees?: string[];
      milestone?: number | null;
    },
  ): Promise<Issue> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/issues/${number}`,
      {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(options),
      },
    );
  }

  async listLabels(name: string): Promise<Label[]> {
    const { labels } = await this.request<{ labels: Label[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/labels`,
    );
    return labels;
  }

  createLabel(
    name: string,
    options: { name: string; color: string; description?: string },
  ): Promise<Label> {
    return this.request(`/api/v1/repos/${encodeURIComponent(name)}/labels`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        name: options.name,
        color: options.color,
        description: options.description ?? "",
      }),
    });
  }

  async listMilestones(name: string): Promise<Milestone[]> {
    const { milestones } = await this.request<{ milestones: Milestone[] }>(
      `/api/v1/repos/${encodeURIComponent(name)}/milestones`,
    );
    return milestones;
  }

  createMilestone(
    name: string,
    options: { title: string; description?: string; due_on?: string },
  ): Promise<Milestone> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/milestones`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          title: options.title,
          description: options.description ?? "",
          due_on: options.due_on,
        }),
      },
    );
  }

  notifications(options?: {
    unread?: boolean;
    limit?: number;
  }): Promise<NotificationList> {
    return this.request(
      `/api/v1/notifications${qs({
        unread: options?.unread ? "true" : undefined,
        limit: options?.limit,
      })}`,
    );
  }

  markNotificationsRead(options?: {
    ids?: number[];
    all?: boolean;
  }): Promise<{ ok: boolean }> {
    return this.request("/api/v1/notifications/mark-read", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        ids: options?.ids ?? [],
        all: options?.all ?? false,
      }),
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
    options?: {
      commitId?: string;
      branch?: string;
      actor?: string;
      workflow?: "ci" | "evaluate" | "inference" | "benchmark";
      releaseVersion?: string;
      /** Stable retry key, scoped by organization and authenticated principal. */
      idempotencyKey?: string;
    },
  ): Promise<ActionRun> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/actions/runs`,
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          ...(options?.idempotencyKey
            ? { "idempotency-key": options.idempotencyKey }
            : {}),
        },
        body: JSON.stringify({
          commit_id: options?.commitId ?? "",
          branch: options?.branch ?? "main",
          actor: options?.actor ?? "manual",
          workflow: options?.workflow ?? "ci",
          release_version: options?.releaseVersion ?? "",
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

  getMergePolicy(name: string): Promise<MergePolicy> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/merge-policy`,
    );
  }

  updateMergePolicy(
    name: string,
    policy: UpdateMergePolicyRequest,
  ): Promise<MergePolicy> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(name)}/merge-policy`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(policy),
      },
    );
  }

  /**
   * List compute providers from the CCI registry (Stage 12).
   * Prefers `/api/v1/providers`; falls back to the Stage 10 path.
   */
  async computeProviders(): Promise<ComputeProvider[]> {
    const list = await this.computeProviderList();
    return list.providers;
  }

  /** Full provider registry response including the default provider id. */
  async computeProviderList(): Promise<ComputeProviderList> {
    try {
      return await this.request<ComputeProviderList>("/api/v1/providers");
    } catch {
      return this.request<ComputeProviderList>("/api/v1/compute/providers");
    }
  }

  async computeProvider(provider: string): Promise<ComputeProvider> {
    try {
      return await this.request<ComputeProvider>(
        `/api/v1/providers/${encodeURIComponent(provider)}`,
      );
    } catch {
      return this.request<ComputeProvider>(
        `/api/v1/compute/providers/${encodeURIComponent(provider)}`,
      );
    }
  }

  /**
   * Provider Fabric list (Stage 17). Pass `layer` for auth/storage/network stubs,
   * or `all: true` for every layer. Omit options for compute-only (same as
   * {@link computeProviderList}).
   */
  listProviders(options?: {
    layer?: ProviderLayer | string;
    all?: boolean;
    org?: string;
  }): Promise<FabricProviderList | ComputeProviderList> {
    return this.request(
      `/api/v1/providers${qs({
        layer: options?.layer,
        all: options?.all ? "true" : undefined,
        org: options?.org,
      })}`,
    );
  }

  getProvider(
    provider: string,
    options?: { layer?: ProviderLayer | string; org?: string },
  ): Promise<FabricProvider | ComputeProvider> {
    return this.request(
      `/api/v1/providers/${encodeURIComponent(provider)}${qs({
        layer: options?.layer,
        org: options?.org,
      })}`,
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
    options?: { displayName?: string; gitOwner?: string },
  ): Promise<Org> {
    return this.request("/api/v1/orgs", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        name,
        display_name: options?.displayName,
        git_owner: options?.gitOwner,
      }),
    });
  }

  getOrg(org: string): Promise<OrgDetail> {
    return this.request(`/api/v1/orgs/${encodeURIComponent(org)}`);
  }

  getOrgReposPage(org: string, options?: RepoPageOptions): Promise<RepoList> {
    return this.request<RepoList>(
      `/api/v1/orgs/${encodeURIComponent(org)}/repos${qs({
        limit: options?.limit,
        cursor: options?.cursor,
      })}`,
    );
  }

  /** Compatibility helper that follows bounded REST pages, up to 10,000 repos. */
  async getOrgRepos(org: string): Promise<RepoInfo[]> {
    return this.collectRepoPages((options) =>
      this.getOrgReposPage(org, options),
    );
  }

  activityPage(options?: ActivityPageOptions): Promise<ActivityPage> {
    return this.request<ActivityPage>(
      `/api/v1/activity${qs({
        limit: options?.limit,
        cursor: options?.cursor,
      })}`,
    );
  }

  /** Compatibility helper returning the events from one explicitly bounded page. */
  async activity(options?: ActivityPageOptions): Promise<ActivityEvent[]> {
    return (await this.activityPage(options)).events;
  }

  me(): Promise<MeResponse> {
    return this.request("/api/v1/me");
  }

  async listTokens(): Promise<ApiTokenMeta[]> {
    const { tokens } = await this.request<{ tokens: ApiTokenMeta[] }>(
      "/api/v1/tokens",
    );
    return tokens;
  }

  createToken(name?: string): Promise<CreatedApiToken> {
    return this.request("/api/v1/tokens", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name: name ?? "" }),
    });
  }

  async revokeToken(id: string): Promise<void> {
    await this.request(`/api/v1/tokens/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  updateRepo(
    name: string,
    options: {
      description?: string;
      visibility?: "public" | "private" | "internal" | string;
      defaultBranch?: string;
      kind?: "code" | "model" | "dataset";
      largeFileThresholdBytes?: number;
      networkMode?: "public" | "tailscale";
      networkTags?: string[];
    },
  ): Promise<RepoDetail> {
    return this.request(`/api/v1/repos/${encodeURIComponent(name)}`, {
      method: "PATCH",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        description: options.description,
        visibility: options.visibility,
        default_branch: options.defaultBranch,
        kind: options.kind,
        large_file_threshold_bytes: options.largeFileThresholdBytes,
        network_mode: options.networkMode,
        network_tags: options.networkTags,
      }),
    });
  }

  async deleteRepo(name: string): Promise<void> {
    await this.request(`/api/v1/repos/${encodeURIComponent(name)}`, {
      method: "DELETE",
    });
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

  // ---------------------------------------------------------------------------
  // Agent identity admin (Slice C, ADR-0016)
  // ---------------------------------------------------------------------------

  async listAgents(): Promise<Agent[]> {
    const { agents } = await this.request<{ agents: Agent[] }>("/api/v1/agents");
    return agents;
  }

  createAgent(options: {
    name: string;
    description?: string;
  }): Promise<Agent> {
    return this.request("/api/v1/agents", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        name: options.name,
        description: options.description ?? "",
      }),
    });
  }

  getAgent(name: string): Promise<AgentDetail> {
    return this.request(`/api/v1/agents/${encodeURIComponent(name)}`);
  }

  mintAgentToken(
    name: string,
    options: {
      allowedRepos: string[];
      allowedTools: string[];
      expiresInSecs?: number;
    },
  ): Promise<MintedAgentToken> {
    return this.request(`/api/v1/agents/${encodeURIComponent(name)}/tokens`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        allowed_repos: options.allowedRepos,
        allowed_tools: options.allowedTools,
        expires_in_secs: options.expiresInSecs,
      }),
    });
  }

  async listAgentTokens(name: string): Promise<AgentTokenMeta[]> {
    const { tokens } = await this.request<{ tokens: AgentTokenMeta[] }>(
      `/api/v1/agents/${encodeURIComponent(name)}/tokens`,
    );
    return tokens;
  }

  async revokeAgentToken(name: string, tokenId: string): Promise<void> {
    await this.request(
      `/api/v1/agents/${encodeURIComponent(name)}/tokens/${encodeURIComponent(tokenId)}`,
      { method: "DELETE" },
    );
  }

  updateAgentTokenScopes(
    name: string,
    tokenId: string,
    options: {
      allowedRepos?: string[];
      allowedTools?: string[];
    },
  ): Promise<AgentTokenMeta> {
    return this.request(
      `/api/v1/agents/${encodeURIComponent(name)}/tokens/${encodeURIComponent(tokenId)}`,
      {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          allowed_repos: options.allowedRepos,
          allowed_tools: options.allowedTools,
        }),
      },
    );
  }

  async agentAudit(
    name: string,
    options?: { limit?: number },
  ): Promise<AgentAuditEntry[]> {
    const { entries } = await this.request<{ entries: AgentAuditEntry[] }>(
      `/api/v1/agents/${encodeURIComponent(name)}/audit${qs({
        limit: options?.limit,
      })}`,
    );
    return entries;
  }

  // ---------------------------------------------------------------------------
  // Secrets (Stage 13, docs/adr/0014) — never returns raw values after write
  // ---------------------------------------------------------------------------

  async orgSecrets(org: string): Promise<SecretMeta[]> {
    const { secrets } = await this.request<{ secrets: SecretMeta[] }>(
      `/api/v1/orgs/${encodeURIComponent(org)}/secrets`,
    );
    return secrets;
  }

  createOrgSecret(
    org: string,
    options: { name: string; value: string; description?: string },
  ): Promise<SecretMeta> {
    return this.request(`/api/v1/orgs/${encodeURIComponent(org)}/secrets`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        name: options.name,
        value: options.value,
        description: options.description ?? "",
      }),
    });
  }

  getOrgSecret(org: string, name: string): Promise<SecretMeta> {
    return this.request(
      `/api/v1/orgs/${encodeURIComponent(org)}/secrets/${encodeURIComponent(name)}`,
    );
  }

  updateOrgSecret(
    org: string,
    name: string,
    options: { value?: string; description?: string },
  ): Promise<SecretMeta> {
    return this.request(
      `/api/v1/orgs/${encodeURIComponent(org)}/secrets/${encodeURIComponent(name)}`,
      {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(options),
      },
    );
  }

  async deleteOrgSecret(org: string, name: string): Promise<void> {
    await this.request(
      `/api/v1/orgs/${encodeURIComponent(org)}/secrets/${encodeURIComponent(name)}`,
      { method: "DELETE" },
    );
  }

  async repoSecrets(repo: string): Promise<SecretMeta[]> {
    const { secrets } = await this.request<{ secrets: SecretMeta[] }>(
      `/api/v1/repos/${encodeURIComponent(repo)}/secrets`,
    );
    return secrets;
  }

  createRepoSecret(
    repo: string,
    options: { name: string; value: string; description?: string },
  ): Promise<SecretMeta> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(repo)}/secrets`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          name: options.name,
          value: options.value,
          description: options.description ?? "",
        }),
      },
    );
  }

  getRepoSecret(repo: string, name: string): Promise<SecretMeta> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(repo)}/secrets/${encodeURIComponent(name)}`,
    );
  }

  updateRepoSecret(
    repo: string,
    name: string,
    options: { value?: string; description?: string },
  ): Promise<SecretMeta> {
    return this.request(
      `/api/v1/repos/${encodeURIComponent(repo)}/secrets/${encodeURIComponent(name)}`,
      {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(options),
      },
    );
  }

  async deleteRepoSecret(repo: string, name: string): Promise<void> {
    await this.request(
      `/api/v1/repos/${encodeURIComponent(repo)}/secrets/${encodeURIComponent(name)}`,
      { method: "DELETE" },
    );
  }

  /**
   * Store a provider API key as an org secret (write-once to the browser).
   * Response is metadata only (masked last4).
   * For `computesdk`, pass `upstream` + `credentials` (env-name keys) for any
   * ComputeSDK provider, or `apiKey` for single-key upstreams.
   */
  connectProvider(
    provider: string,
    options: {
      apiKey?: string;
      clientId?: string;
      clientSecret?: string;
      org?: string;
      upstream?: string;
      credentials?: Record<string, string>;
      modalTokenId?: string;
      modalTokenSecret?: string;
    },
  ): Promise<SecretMeta> {
    return this.request(
      `/api/v1/providers/${encodeURIComponent(provider)}/connect`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          api_key: options.apiKey ?? "",
          client_id: options.clientId ?? "",
          client_secret: options.clientSecret ?? "",
          org: options.org ?? "",
          upstream: options.upstream ?? "",
          credentials: options.credentials ?? {},
          modal_token_id: options.modalTokenId ?? "",
          modal_token_secret: options.modalTokenSecret ?? "",
        }),
      },
    );
  }

  /**
   * Catalog of ComputeSDK upstream providers and required secret names.
   * @see https://docs.computesdk.com/providers.md
   */
  listComputesdkUpstreams(): Promise<{
    upstreams: Array<{
      id: string;
      name: string;
      pkg: string;
      required: string[];
      optional?: string[];
      notes?: string;
    }>;
  }> {
    return this.request("/api/v1/providers/computesdk/upstreams");
  }

  /**
   * Remove Clotho-stored credentials for a provider (metadata only).
   * Does not return secret values.
   */
  disconnectProvider(
    provider: string,
    options?: { org?: string },
  ): Promise<{ provider: string; deleted_secrets: string[] }> {
    const q = qs({ org: options?.org });
    return this.request(
      `/api/v1/providers/${encodeURIComponent(provider)}/connect${q}`,
      { method: "DELETE" },
    );
  }

  private async collectRepoPages(
    load: (options: RepoPageOptions) => Promise<RepoList>,
  ): Promise<RepoInfo[]> {
    const repos: RepoInfo[] = [];
    const seen = new Set<string>();
    let cursor: string | undefined;
    for (let page = 0; page < 100; page += 1) {
      const response = await load({ limit: 100, cursor });
      repos.push(...response.repos);
      const next = response.next_cursor ?? undefined;
      if (!next) return repos;
      if (seen.has(next)) {
        throw new Error("repository pagination returned a repeated cursor");
      }
      seen.add(next);
      cursor = next;
    }
    throw new Error("repository pagination exceeded 10,000 items");
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
