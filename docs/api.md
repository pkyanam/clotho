# Clotho REST API

The **api-gateway** (`clotho-api-gateway`, default `http://localhost:8080`) is
the single product contract for humans (CLI), agents (MCP tools that call this
edge), the web console, and `@clotho/sdk-js`.

## Contract

| Artifact | Location |
|---|---|
| OpenAPI 3 | [`docs/openapi.yaml`](openapi.yaml) |
| Live YAML | `GET /openapi.yaml` on a running gateway |
| Drift CI | `crates/clotho-api-gateway/tests/openapi_drift.rs` |
| Error envelope | `{ "error": "…" }` with 4xx/5xx |

### Auth model (Stage 17 / ADR-0018)

Human authentication is pluggable behind `AuthProvider`:

| Provider | When | Credentials |
|---|---|---|
| `bootstrap` (default) | Local / `just demo` / CI | `clotho_tok_…` Bearer; open fallback when `CLOTHO_AUTH_REQUIRED=false` |
| `clerk` | Managed / production | Clerk session JWT or Clerk org API key; also accepts Clotho `clotho_tok_…` (§11 #7) |

Agents **never** use Clerk. Agent MCP credentials remain scoped `clotho_agt_…`
tokens (ADR-0005), minted only via human-only agent admin (ADR-0016).

| Env / flag | Purpose |
|---|---|
| `CLOTHO_AUTH_PROVIDER` | `bootstrap` (default) or `clerk` |
| `CLOTHO_AUTH_REQUIRED` | `true` to require Bearer (managed default; local default `false`) |
| `CLOTHO_TOKEN` | SDK, CLI (`--token`), web server bootstrap path |
| `CLOTHO_BOOTSTRAP_TOKEN` | Deterministic bootstrap token on first start |
| `CLERK_PUBLISHABLE_KEY` / `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` | Web Clerk UI (managed only) |
| `CLERK_SECRET_KEY` | Gateway Clerk Backend / verification (managed only) |
| `CLERK_JWKS_URL` | Clerk JWKS for session JWT verification |
| `CLOTHO_CLERK_JWT_SECRET` | Dev/test HS256 secret (mocks; not for production) |

On first gateway start with Postgres, read the bootstrap token from gateway logs
or set `CLOTHO_BOOTSTRAP_TOKEN` before boot. Mint additional Clotho human tokens
via `POST /api/v1/tokens` or `clotho auth token create`. Inspect the current
actor with `GET /api/v1/me`.

**§11 #7 default:** Clotho continues minting `clotho_tok_…` under both
providers. Under `clerk`, Clerk session JWTs and org API keys also resolve to
the same Clotho principal + permission checks.

Agent MCP credentials are scoped tokens (`clotho_agt_…`) minted through the
**REST edge** agent admin API (ADR-0016) — not via MCP tools.
Secrets are encrypted at rest (ADR-0014); raw values are **never**
returned after write — only metadata + `value_last4`.

### Provider Fabric (Stage 17 skeleton)

`GET /api/v1/providers` remains the compute registry (backward compatible).
Filter by fabric layer:

```bash
curl -s 'http://localhost:8080/api/v1/providers?layer=auth'
curl -s 'http://localhost:8080/api/v1/providers?layer=storage'   # stub until Stage 18
curl -s 'http://localhost:8080/api/v1/providers?layer=network'   # stub until Stage 19
curl -s 'http://localhost:8080/api/v1/providers?all=true'
```

Storage/network entries report `configured: false` honestly until their stages.

## Quick start

```bash
# health
curl -s http://localhost:8080/healthz | jq

# whoami (with token)
export CLOTHO_TOKEN=clotho_tok_…
curl -s http://localhost:8080/api/v1/me \
  -H "Authorization: Bearer $CLOTHO_TOKEN" | jq

# create a repo
curl -s -X POST http://localhost:8080/api/v1/repos \
  -H 'content-type: application/json' \
  -H "Authorization: Bearer $CLOTHO_TOKEN" \
  -d '{"name":"weave"}' | jq

# update repo settings
curl -s -X PATCH http://localhost:8080/api/v1/repos/weave \
  -H 'content-type: application/json' \
  -H "Authorization: Bearer $CLOTHO_TOKEN" \
  -d '{"description":"demo repo","visibility":"private"}' | jq

# open an issue with labels and assignee
curl -s -X POST http://localhost:8080/api/v1/repos/weave/issues \
  -H 'content-type: application/json' \
  -H "Authorization: Bearer $CLOTHO_TOKEN" \
  -d '{"title":"flaky test","body":"repro on main","labels":["bug"],"assignees":["clotho"]}' | jq

# list notifications
curl -s http://localhost:8080/api/v1/notifications?unread=true \
  -H "Authorization: Bearer $CLOTHO_TOKEN" | jq

# merge policy (Slice E)
curl -s http://localhost:8080/api/v1/repos/weave/merge-policy \
  -H "Authorization: Bearer $CLOTHO_TOKEN" | jq

# start an Action run
curl -s -X POST http://localhost:8080/api/v1/repos/weave/actions/runs \
  -H 'content-type: application/json' \
  -H "Authorization: Bearer $CLOTHO_TOKEN" \
  -d '{"actor":"docs"}' | jq

# list providers (honest configured flags)
curl -s http://localhost:8080/api/v1/providers | jq
```

## Surface map

| Area | Prefix |
|---|---|
| Auth | `/api/v1/me`, `/api/v1/tokens` |
| Control plane | `/api/v1/users`, `/orgs`, `/activity` |
| Repos / VCS | `/api/v1/repos`, `…/tree`, `…/file`, `…/commits`, `…/submit` |
| Repo settings | `PATCH /api/v1/repos/{name}`, `DELETE /api/v1/repos/{name}` |
| Issues | `…/issues`, `…/issues/{n}` (PATCH), `…/issues/{n}/comments` |
| Labels / milestones | `…/labels`, `…/milestones` (Slice D) |
| Notifications | `/api/v1/notifications`, `…/mark-read` (Slice D) |
| Pulls | `…/pulls`, comments, reviews, merge, diff |
| Merge policy | `GET/PUT …/merge-policy` (Slice E, ADR-0017) |
| Actions | `…/actions/runs`, `…/logs`, `…/config` |
| Providers | `/api/v1/providers`, `…/connect` (POST connect / DELETE disconnect) |
| Secrets | `/api/v1/orgs/{org}/secrets`, `/api/v1/repos/{repo}/secrets` |
| Agents (presence) | `…/agent-sessions` |
| Agents (admin) | `/api/v1/agents`, `…/tokens`, `…/audit` |

### Human tokens

| Method | Path | Notes |
|---|---|---|
| `GET` | `/api/v1/me` | Current user + permissions |
| `GET` | `/api/v1/tokens` | Metadata only (prefix, name) |
| `POST` | `/api/v1/tokens` | Mint; plaintext `token` once |
| `DELETE` | `/api/v1/tokens/{id}` | Revoke |

### Agents admin (Slice C)

Requires human Bearer auth. Caller must be bootstrap user or **org admin**.
The api-gateway proxies to agent-gateway with `CLOTHO_AGENT_ADMIN_TOKEN`; if
that env is unset, admin routes return **503** (`agent management is not
configured`).

| Method | Path | Notes |
|---|---|---|
| `GET` | `/api/v1/agents` | List identities |
| `POST` | `/api/v1/agents` | `{ name, description? }` |
| `GET` | `/api/v1/agents/{name}` | Detail + token metadata |
| `POST` | `/api/v1/agents/{name}/tokens` | Mint; plaintext `token` once |
| `GET` | `/api/v1/agents/{name}/tokens` | Metadata only |
| `DELETE` | `/api/v1/agents/{name}/tokens/{token_id}` | Revoke |
| `PATCH` | `/api/v1/agents/{name}/tokens/{token_id}` | `{ allowed_repos?, allowed_tools? }` |
| `GET` | `/api/v1/agents/{name}/audit` | `{ entries: [...] }` |

```bash
curl -s http://localhost:8080/api/v1/agents \
  -H "Authorization: Bearer $CLOTHO_TOKEN" | jq

curl -s -X POST http://localhost:8080/api/v1/agents/weaver/tokens \
  -H "Authorization: Bearer $CLOTHO_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"allowed_repos":["weave"],"allowed_tools":["*"]}' | jq
# → { "token": "clotho_agt_…", … }  (shown once)
```

### Labels, milestones, notifications (Slice D)

| Method | Path | Notes |
|---|---|---|
| `GET/POST` | `/api/v1/repos/{name}/labels` | List / create label |
| `GET/POST` | `/api/v1/repos/{name}/milestones` | List / create milestone |
| `GET` | `/api/v1/notifications` | `?unread=true` optional |
| `POST` | `/api/v1/notifications/mark-read` | `{ ids: [...] }` or mark all |

Issue and PR create/update bodies accept `labels`, `assignees`, and `milestone`
where applicable.

### Merge policy (Slice E)

| Method | Path | Notes |
|---|---|---|
| `GET` | `/api/v1/repos/{name}/merge-policy` | Current gates |
| `PUT` | `/api/v1/repos/{name}/merge-policy` | Update gates |

`POST …/pulls/{n}/merge` returns **409** with `{ "error": "…" }` when policy
blocks merge (conflicts, missing approvals, failing Actions, etc.).

| Webhooks | `/api/v1/webhooks/forgejo` (internal; not part of the public product API) |

Full schemas and request bodies: **[`openapi.yaml`](openapi.yaml)**.

## SDK

```ts
import { ClothoClient } from "@clotho/sdk-js";

const clotho = new ClothoClient({
  baseUrl: "http://localhost:8080",
  token: process.env.CLOTHO_TOKEN,
});
const issue = await clotho.createIssue("weave", {
  title: "flaky test",
  body: "repro on main",
  labels: ["bug"],
});
const run = await clotho.createActionRun("weave", { actor: "script" });
const logs = await clotho.actionLogs("weave", run.id);
```

## Versioning

- Additive REST changes are fine within the prototype (`0.x`).
- Breaking changes require an explicit major bump once Clotho leaves prototype.
- Prefer documenting new paths in OpenAPI **in the same PR** as the Axum route
  (the drift test enforces path presence).
