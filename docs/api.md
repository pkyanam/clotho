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

**Auth (prototype):** most `/api/v1/*` routes are open (dev-friendly). Agent
identity lives on the **agent-gateway** (`/admin/v1/*` + bearer tokens for
MCP). Secrets are encrypted at rest (ADR-0014); raw values are **never**
returned after write — only metadata + `value_last4`.

## Quick start

```bash
# health
curl -s http://localhost:8080/healthz | jq

# create a repo
curl -s -X POST http://localhost:8080/api/v1/repos \
  -H 'content-type: application/json' \
  -d '{"name":"weave"}' | jq

# open an issue
curl -s -X POST http://localhost:8080/api/v1/repos/weave/issues \
  -H 'content-type: application/json' \
  -d '{"title":"flaky test","body":"repro on main"}' | jq

# start an Action run
curl -s -X POST http://localhost:8080/api/v1/repos/weave/actions/runs \
  -H 'content-type: application/json' \
  -d '{"actor":"docs"}' | jq

# list providers (honest configured flags)
curl -s http://localhost:8080/api/v1/providers | jq
```

## Surface map

| Area | Prefix |
|---|---|
| Control plane | `/api/v1/users`, `/orgs`, `/activity` |
| Repos / VCS | `/api/v1/repos`, `…/tree`, `…/file`, `…/commits`, `…/submit` |
| Issues | `…/issues`, `…/issues/{n}/comments` |
| Pulls | `…/pulls`, comments, reviews, merge, diff |
| Actions | `…/actions/runs`, `…/logs`, `…/config` |
| Providers | `/api/v1/providers`, `…/connect` (POST connect / DELETE disconnect) |
| Secrets | `/api/v1/orgs/{org}/secrets`, `/api/v1/repos/{repo}/secrets` |
| Agents (presence) | `…/agent-sessions` |
| Webhooks | `/api/v1/webhooks/forgejo` (internal) |

Full schemas and request bodies: **[`openapi.yaml`](openapi.yaml)**.

## SDK

```ts
import { ClothoClient } from "@clotho/sdk-js";

const clotho = new ClothoClient({ baseUrl: "http://localhost:8080" });
const issue = await clotho.createIssue("weave", {
  title: "flaky test",
  body: "repro on main",
});
const run = await clotho.createActionRun("weave", { actor: "script" });
const logs = await clotho.actionLogs("weave", run.id);
```

## Versioning

- Additive REST changes are fine within the prototype (`0.x`).
- Breaking changes require an explicit major bump once Clotho leaves prototype.
- Prefer documenting new paths in OpenAPI **in the same PR** as the Axum route
  (the drift test enforces path presence).
