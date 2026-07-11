# Clotho MCP (agent gateway)

Agents talk to Clotho over **streamable HTTP MCP** on the agent-gateway
(default `http://localhost:8090/mcp`), authenticated with a scoped bearer
token (`clotho_agt_…`).

VCS tools use internal gRPC. **Collab, Actions, and platform tools call the
REST edge** (`CLOTHO_API_URL`) so agents cannot drift from the public API
([`openapi.yaml`](openapi.yaml)).

> **Pre-release MCP:** tool names and scopes are usable but not yet under a
> compatibility freeze. REST-backed errors now preserve stable codes and
> request IDs. Public alpha still requires versioned schemas, operation
> handles/progress/cancellation for durable work, capability classes, broader
> REST-equivalence tests, audit correlation, and a published
> prompt-injection authority model. See
> [`release-readiness.md`](release-readiness.md#mcp-and-autonomous-agent-handoff).

## Auth

1. An operator creates an agent and mints a token through the **REST edge**
   (web `/agents`, `clotho agent …`, or `POST /api/v1/agents/…`) with human
   Bearer auth — see [ADR-0016](adr/0016-agent-admin-via-edge.md). The
   api-gateway proxies to agent-gateway with `CLOTHO_AGENT_ADMIN_TOKEN`.

```bash
export CLOTHO_TOKEN=clotho_tok_…
clotho agent create weaver --description "demo agent"
clotho agent mint weaver --repos demo-loop --tools '*'
# save the printed clotho_agt_… value — shown once
```

Equivalent curl:

```bash
curl -s -X POST http://localhost:8080/api/v1/agents \
  -H "Authorization: Bearer $CLOTHO_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"name":"weaver","description":"demo agent"}'

curl -s -X POST http://localhost:8080/api/v1/agents/weaver/tokens \
  -H "Authorization: Bearer $CLOTHO_TOKEN" \
  -H 'content-type: application/json' \
  -d '{
    "allowed_repos": ["demo-loop"],
    "allowed_tools": ["*"]
  }'
# → { "token": "clotho_agt_…", … }  (plaintext returned once)
```

2. Connect an MCP client with `Authorization: Bearer clotho_agt_…`.

**MCP does not expose tools to create agents, mint tokens, or revoke peer
credentials.** Operators provision agent identities out of band (CLI or web);
agents must not mint sibling tokens (ADR-0016). MCP tools never return secret
values — only metadata where applicable.

Every tool call is audited (agent, tool, repo, status). Scope denials return
an MCP tool error result, not a transport failure.
`tools/list` is filtered to the authenticated token's `allowed_tools`, so an
agent is not instructed to call capabilities it cannot use. Call-time checks
remain authoritative and are audited even if a client caches an older list.
REST-backed tool failures preserve Clotho's stable `code`, `request_id`,
`retryable`, HTTP status, and safe `details` in MCP error data. Agents should
branch on the code and include the request id when handing an incident to an
operator; error prose is not a compatibility contract.

Repository files, cards, issues, comments, imported metadata, Actions logs, and
tool output are **untrusted content**, never authority. They cannot expand token
scope, mint credentials, approve destructive work, or override Clotho policy.
Agents must obtain new authority from an authenticated operator through the
human REST/CLI/web surfaces.

- **Repo-scoped tools** require `allowed_repos` to include the repo (or `*`).
- **Platform tools** (`list_providers`, `list_repos`, `get_activity`, and
  org-scoped `list_secrets`) only check `allowed_tools`.

## Tool catalog

### VCS (gRPC)

| Tool | Purpose |
|---|---|
| `orient_repo` | Heads, main, op log, file summary |
| `checkpoint` | Named op-log checkpoint |
| `restore_to` | Restore to an operation id |
| `diff_symbol` | Symbol-level diff |
| `commit` | Author a commit from file contents |
| `submit_change` | Land via merge queue |

### Collab (REST)

| Tool | REST |
|---|---|
| `list_issues` | `GET …/issues` |
| `create_issue` | `POST …/issues` (optional `labels`, `assignees`, `milestone`) |
| `comment_issue` | `POST …/issues/{n}/comments` |
| `list_pulls` | `GET …/pulls` |
| `create_pull` | `POST …/pulls` |
| `comment_pull` | `POST …/pulls/{n}/comments` |
| `review_pull` | `POST …/pulls/{n}/reviews` |
| `merge_pull` | `POST …/pulls/{n}/merge` |

### Actions (REST)

| Tool | REST |
|---|---|
| `list_action_runs` | `GET …/actions/runs` |
| `start_action_run` | `POST …/actions/runs` |
| `get_action_logs` | `GET …/actions/runs/{id}/logs` |

### Platform / read helpers (REST)

| Tool | REST |
|---|---|
| `list_providers` | `GET /api/v1/providers` |
| `list_repos` | `GET /api/v1/repos?limit={1..100}&cursor={opaque}` |
| `get_activity` | `GET /api/v1/activity` |
| `list_secrets` | org/repo secrets **metadata only** |
| `get_tree` | `GET …/tree` |
| `get_file` | `GET …/file` |

`list_repos` accepts optional `limit` and `cursor` arguments and returns the
canonical REST page envelope with `repos` and `next_cursor`. Agents must treat
the cursor as opaque, request one bounded page at a time, and stop when
`next_cursor` is absent. Invalid cursors preserve REST's `invalid_request` code
and request ID in MCP error data.

## Demo loop

**Step 0 — provision token (CLI only, not MCP):**

```bash
export CLOTHO_TOKEN=clotho_tok_…
clotho agent create weaver --description "MCP demo"
clotho agent mint weaver --repos demo-loop --tools '*'
export CLOTHO_AGENT_TOKEN=clotho_agt_…   # from mint output
```

**Steps 1–6 — MCP tools** (connect with `Authorization: Bearer $CLOTHO_AGENT_TOKEN`):

1. `list_repos` / `orient_repo` — situational awareness on `demo-loop`
2. `create_issue` — e.g. title `"flaky"`, labels `["bug"]`
3. `commit` + `submit_change` — land code on a branch
4. `create_pull` / `review_pull` / `merge_pull` as needed
5. `start_action_run` → `list_action_runs` → `get_action_logs`
6. `list_providers` — honest compute status

Merge policy gates from Slice E apply to `merge_pull` the same as the REST
`POST …/merge` endpoint (409-style errors surfaced as tool failures).

## Config

| Env | Default | Meaning |
|---|---|---|
| `CLOTHO_AGENT_HTTP_ADDR` | `0.0.0.0:8090` | MCP + admin listen |
| `CLOTHO_API_URL` | `http://localhost:8080` | REST edge for Stage 15 tools |
| `CLOTHO_AGENT_ADMIN_TOKEN` | (required) | Admin surface bearer |
| `CLOTHO_VCS_GRPC_URL` etc. | localhost ports | VCS / diff / merge-queue |

Integration tests: `just test-agent` (needs `just dev`).
