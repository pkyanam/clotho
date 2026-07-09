# Clotho MCP (agent gateway)

Agents talk to Clotho over **streamable HTTP MCP** on the agent-gateway
(default `http://localhost:8090/mcp`), authenticated with a scoped bearer
token (`clotho_agt_…`).

VCS tools use internal gRPC. **Collab, Actions, and platform tools call the
REST edge** (`CLOTHO_API_URL`) so agents cannot drift from the public API
([`openapi.yaml`](openapi.yaml)).

## Auth

1. Admin creates an agent and mints a token (admin bearer =
   `CLOTHO_AGENT_ADMIN_TOKEN`):

```bash
curl -s -X POST http://localhost:8090/admin/v1/agents \
  -H "authorization: Bearer clotho-agent-admin-dev" \
  -H 'content-type: application/json' \
  -d '{"name":"weaver","description":"demo agent"}'

curl -s -X POST http://localhost:8090/admin/v1/agents/weaver/tokens \
  -H "authorization: Bearer clotho-agent-admin-dev" \
  -H 'content-type: application/json' \
  -d '{
    "allowed_repos": ["my-demo"],
    "allowed_tools": ["*"]
  }'
# → { "token": "clotho_agt_…", … }  (plaintext returned once)
```

2. Connect an MCP client with `Authorization: Bearer clotho_agt_…`.

Every tool call is audited (agent, tool, repo, status). Scope denials return
an MCP tool error result, not a transport failure.

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
| `create_issue` | `POST …/issues` |
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
| `list_repos` | `GET /api/v1/repos` |
| `get_activity` | `GET /api/v1/activity` |
| `list_secrets` | org/repo secrets **metadata only** |
| `get_tree` | `GET …/tree` |
| `get_file` | `GET …/file` |

## Demo loop (MCP only)

With a token allowed on `my-demo` and tools `*`:

1. `list_repos` / `orient_repo` — situational awareness  
2. `create_issue` — open work  
3. `commit` + `submit_change` — land code  
4. `create_pull` / `review_pull` / `merge_pull` as needed  
5. `start_action_run` → `list_action_runs` → `get_action_logs`  
6. `list_providers` — honest compute status  

## Config

| Env | Default | Meaning |
|---|---|---|
| `CLOTHO_AGENT_HTTP_ADDR` | `0.0.0.0:8090` | MCP + admin listen |
| `CLOTHO_API_URL` | `http://localhost:8080` | REST edge for Stage 15 tools |
| `CLOTHO_AGENT_ADMIN_TOKEN` | (required) | Admin surface bearer |
| `CLOTHO_VCS_GRPC_URL` etc. | localhost ports | VCS / diff / merge-queue |

Integration tests: `just test-agent` (needs `just dev`).
