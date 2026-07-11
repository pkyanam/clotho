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
curl -s 'http://localhost:8080/api/v1/providers?layer=storage'   # live Arachne + StorageSDK state
curl -s 'http://localhost:8080/api/v1/providers?layer=network'   # live Tailscale probe
curl -s 'http://localhost:8080/api/v1/providers?layer=hub'       # Hub import providers
curl -s 'http://localhost:8080/api/v1/providers?all=true'
```

Storage reports live Arachne capacity and the optional StorageSDK bridge state.
Network entries remain unconfigured until a provider is connected.

Connect Tailscale with an OAuth client that has `auth_keys` scope. Clotho
requests a short-lived access token to verify the client before encrypting the
credentials; invalid clients are never stored:

```bash
curl -s -X POST http://localhost:8080/api/v1/providers/tailscale/connect \
  -H 'content-type: application/json' \
  -H "Authorization: Bearer $CLOTHO_TOKEN" \
  -d '{"org":"clotho","client_id":"…","client_secret":"…"}' | jq
```

Set repository network intent through the same source-of-truth API:

```bash
curl -s -X PATCH http://localhost:8080/api/v1/repos/weave \
  -H 'content-type: application/json' \
  -d '{"network_mode":"tailscale","network_tags":["tag:clotho-weave"]}' | jq
```

GPU runner policy is stored in the repo's Actions config. The provider must
advertise GPU support; unsupported providers/types are rejected before save:

```bash
curl -s -X PUT http://localhost:8080/api/v1/repos/weave/actions/config \
  -H 'content-type: application/json' \
  -d '{"enabled":true,"provider":"daytona","accelerator":"gpu","gpu_types":["H100","H200"],"default_image":"","timeout_seconds":1800}' | jq
```

For Daytona, Clotho maps GPU intent to `daytona-gpu`. The preferences are also
injected as `CLOTHO_GPU_TYPES` for job provenance and workflow decisions.

Model and dataset repositories expose a logical, provider-independent artifact
manifest. It classifies portable model weights, tokenizers, dataset shards,
schemas, cards, and evaluations without downloading large Arachne payloads:

```bash
curl -s http://localhost:8080/api/v1/repos/weave/artifacts | jq
clotho repo artifacts weave
curl -s 'http://localhost:8080/api/v1/repos/my-dataset/artifacts/preview?path=data/train.jsonl&limit=25' | jq
```

The response includes logical sizes, Git/Arachne placement, format and role
counts, and publication-readiness warnings. This is Clotho-owned metadata;
Forgejo remains an implementation detail.

Hugging Face-compatible YAML frontmatter in `README.md` is exposed as
structured `metadata` (license, language, task, library, datasets, tags, base
model, metrics, and version links). Selected portable fields from `config.json`
and `dataset_info.json` are composed into the same manifest, with
`metadata_sources` preserving provenance. Parsing is bounded and ignores
unknown or nested YAML rather than executing a general-purpose YAML runtime.

Import a public Hugging Face snapshot directly into an existing Clotho model
or dataset repository. The source host and paths are validated, Hub scanner
results fail closed, large responses stream directly to Arachne, and the
result is committed/submitted through Clotho VCS:

```bash
curl -s -X POST http://localhost:8080/api/v1/repos/tiny-gpt/hub-imports \
  -H 'content-type: application/json' \
  -d '{"repo_id":"hf-internal-testing/tiny-random-gpt2","revision":"main"}' | jq
curl -s http://localhost:8080/api/v1/repos/tiny-gpt/hub-imports | jq
```

Public imports need no credential. For private/gated sources, connect a token
in Clotho (`provider connect huggingface`) or store `HUGGINGFACE_TOKEN` as an
org/repo secret; environment configuration is not required.
Imports run as durable control-plane jobs. Clotho persists preflight totals,
per-file byte progress, Arachne counts, scanner summaries, commits, and terminal
errors; the web app polls live state. Queued/running jobs are replayed after a
gateway restart, with content-addressed uploads deduplicating completed work.
Thirty-second database leases are renewed by worker heartbeats; an expired job
is reclaimed automatically, while a healthy worker cannot be stolen by another
gateway replica.

Create an immutable, tamper-evident release from the current main commit. Model
and dataset releases fail closed unless their card, primary artifact, and
structured metadata are publishable:

```bash
curl -s -X POST http://localhost:8080/api/v1/repos/tiny-gpt/releases \
  -H 'content-type: application/json' \
  -d '{"version":"v1.0.0"}' | jq
curl -s http://localhost:8080/api/v1/repos/tiny-gpt/releases/v1.0.0 | jq
```

The frozen manifest is bound to its Git commit and SHA-256 digest. Every read
recomputes the digest and exposes `verified`; release versions cannot be
overwritten or deleted.

Release-pinned GPU workflows resolve that immutable commit and fail closed on
missing, incomplete, or tampered releases:

```bash
curl -s -X POST http://localhost:8080/api/v1/repos/tiny-gpt/actions/runs \
  -H 'content-type: application/json' \
  -d '{"workflow":"evaluate","release_version":"v1.0.0"}' | jq
```

The sandbox receives `CLOTHO_WORKFLOW`, `CLOTHO_COMMIT_ID`,
`CLOTHO_RELEASE_VERSION`, and `CLOTHO_RELEASE_MANIFEST_SHA256`, then executes
`.clotho/evaluate.sh`, `.clotho/inference.sh`, or `.clotho/benchmark.sh`.

CSV, TSV, and JSONL previews are deliberately bounded: the gateway streams at
most 256 KiB from Arachne and returns at most 100 rows. Large datasets never
need to be materialized in gateway memory just to inspect their shape.

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
  -d '{"name":"weave","kind":"model"}' | jq

# update repo settings
curl -s -X PATCH http://localhost:8080/api/v1/repos/weave \
  -H 'content-type: application/json' \
  -H "Authorization: Bearer $CLOTHO_TOKEN" \
  -d '{"description":"demo repo","visibility":"private","large_file_threshold_bytes":1048576}' | jq

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
| Repos / VCS | `/api/v1/repos`, `…/tree`, `…/artifacts`, `…/file`, `…/commits`, `…/submit` |
| Repo settings | `PATCH /api/v1/repos/{name}`, `DELETE /api/v1/repos/{name}` |
| Issues | `…/issues`, `…/issues/{n}` (PATCH), `…/issues/{n}/comments` |
| Labels / milestones | `…/labels`, `…/milestones` (Slice D) |
| Notifications | `/api/v1/notifications`, `…/mark-read` (Slice D) |
| Pulls | `…/pulls`, comments, reviews, merge, diff |
| Merge policy | `GET/PUT …/merge-policy` (Slice E, ADR-0017) |
| Actions | `…/actions/runs`, `…/logs`, `…/config` |
| Providers | `/api/v1/providers`, `…/connect` (POST connect / DELETE disconnect), repo `…/imports/huggingface` |
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
