# Clotho REST API

The **api-gateway** (`clotho-api-gateway`, default `http://localhost:8080`) is
the single product contract for humans (CLI), agents (MCP tools that call this
edge), the web console, and `@clotho/sdk-js`.

> **Pre-release contract:** `/api/v1` is the intended compatibility boundary,
> but the public-alpha hardening gate is not complete. Consumers should pin a
> Clotho release until pagination across every unbounded collection,
> idempotency across every retryable mutation, and async-operation conventions
> land. Error envelope version `1`, request correlation, structural OpenAPI/SDK
> verification, repository/activity pagination, and persisted manual Action-run
> idempotency are stable. Track the remaining contract in
> [`release-readiness.md`](release-readiness.md).

## Contract

| Artifact                     | Location                                                                      |
| ---------------------------- | ----------------------------------------------------------------------------- |
| OpenAPI 3                    | [`docs/openapi.yaml`](openapi.yaml)                                           |
| Live YAML                    | `GET /openapi.yaml` on a running gateway                                      |
| Structural verification      | `pnpm test:contract` (`scripts/verify-api-contract.mjs`)                      |
| Machine inventory            | `pnpm test:contract -- --json`                                                |
| Embedded-contract smoke test | `crates/clotho-api-gateway/tests/openapi_drift.rs`                            |
| Error envelope               | version `1`: `code`, `message`, `request_id`, `retryable`, optional `details` |

Contract verification parses the YAML, resolves every internal `$ref`, and
requires unique operation IDs, summaries, mutation request bodies, explicit
success schemas, declared path parameters, and effective stability/auth/error
metadata. It compares the complete Axum HTTP method/path inventory to OpenAPI,
then verifies canonical SDK endpoint coverage and matching component property
names, requiredness, and base types. The JSON form is deterministic and is the
input for release API diffs.

Every operation inherits the top-level `x-clotho-contract` alpha defaults
unless it declares a narrower `x-clotho-auth` or `x-clotho-stability` value.
This records the local bootstrap-or-bearer posture and the common versioned
error response without pretending the alpha API is stable.

Every response includes `X-Request-Id`. A caller may supply a 1–128 character
opaque id containing ASCII letters, digits, `.`, `_`, or `-`; invalid values
are replaced with a generated UUID. Logs use the same id. Never put secrets or
user content in a request id.

Error responses are safe and machine-readable:

```json
{
  "version": "1",
  "code": "permission_denied",
  "message": "requires repo write permission",
  "request_id": "0190f3b6-3b44-7d4c-a180-2f87ef7e20f1",
  "retryable": false
}
```

Stable codes in envelope version `1` are `invalid_request`,
`unauthenticated`, `permission_denied`, `not_found`, `method_not_allowed`,
`conflict`, `idempotency_conflict`, `policy_conflict`, `payload_too_large`,
`range_not_satisfiable`, `rate_limited`, `upstream_unavailable`,
`service_unavailable`, `upstream_timeout`, and `internal_error`. Internal and
provider topology is logged server-side but never returned in the safe message.

### Repository pagination

`GET /api/v1/repos` and `GET /api/v1/orgs/{org}/repos` use the same bounded
cursor contract:

- `limit` defaults to `100` and must be between `1` and `100`.
- `cursor` is an opaque, URL-safe continuation token. Clients must not decode,
  alter, or persist assumptions about its contents.
- responses are `{ "repos": [...], "next_cursor": string | null }` and contain
  at most `limit` repositories.
- ordering is deterministic: newest `updated_at` first, then name ascending.
- a missing `next_cursor` means the collection is exhausted. An invalid or
  oversized cursor fails closed with `400 invalid_request`.

The JavaScript SDK exposes `listReposPage` and `getOrgReposPage` for automation.
Its compatibility `listRepos` and `getOrgRepos` helpers follow bounded pages,
with loop and total-result guards. The CLI and MCP surfaces return one page so
callers retain explicit control of work and latency.

### Activity pagination

`GET /api/v1/activity` uses a bounded keyset cursor over immutable
`(created_at, id)` ordering:

- `limit` defaults to `50` and must be between `1` and `100`.
- responses are `{ "events": [...], "next_cursor": string | null }`.
- `cursor` is opaque and limited to 2,048 characters; invalid cursors fail with
  `400 invalid_request` rather than restarting at the first page.
- newest events are returned first, with the event id providing a deterministic
  tie-breaker when timestamps match.

The SDK's canonical `activityPage` method, CLI `activity --cursor`, MCP
`get_activity`, and web activity page preserve this envelope and cursor.

### Manual Action-run idempotency

`POST /api/v1/repos/{name}/actions/runs` accepts an optional
`Idempotency-Key` header for safe retries after a timeout or lost response:

- keys are opaque, 1–128 ASCII characters, and may contain letters, digits,
  `.`, `_`, `:`, or `-`;
- a key is scoped to the immutable organization and authenticated principal,
  hashed before storage, and retained for 24 hours;
- the first accepted request atomically persists the key, queued Action run,
  initial log, and exact `202` response before compute starts;
- retrying the same semantic request returns the original run and does not
  schedule duplicate compute;
- reusing the key with a different repo, requested commit, branch, actor,
  workflow, or release returns `409 idempotency_conflict`;
- `Idempotency-Replayed: false` identifies the first acceptance and
  `Idempotency-Replayed: true` identifies a persisted replay.

For a branch-based request with no explicit commit, the original run remains
the result of that key even if the branch moves before a retry. Generate a new
key for new work; never derive a key from a secret.

```bash
curl -s -X POST http://localhost:8080/api/v1/repos/weave/actions/runs \
  -H 'content-type: application/json' \
  -H 'Idempotency-Key: action-20260711-01' \
  -H "Authorization: Bearer $CLOTHO_TOKEN" \
  -d '{"actor":"automation","workflow":"ci"}' | jq
```

This is the first common persisted-idempotency slice. Other retryable
create/start/import/submit routes remain outside this guarantee until their
OpenAPI operations explicitly declare the same contract.

### Auth model (Stage 17 / ADR-0018)

Human authentication is pluggable behind `AuthProvider`:

| Provider              | When                     | Credentials                                                                         |
| --------------------- | ------------------------ | ----------------------------------------------------------------------------------- |
| `bootstrap` (default) | Local / `just demo` / CI | `clotho_tok_…` Bearer; open fallback when `CLOTHO_AUTH_REQUIRED=false`              |
| `clerk`               | Managed / production     | Clerk session JWT or Clerk org API key; also accepts Clotho `clotho_tok_…` (§11 #7) |

Agents **never** use Clerk. Agent MCP credentials remain scoped `clotho_agt_…`
tokens (ADR-0005), minted only via human-only agent admin (ADR-0016).

| Env / flag                                                    | Purpose                                                           |
| ------------------------------------------------------------- | ----------------------------------------------------------------- |
| `CLOTHO_AUTH_PROVIDER`                                        | `bootstrap` (default) or `clerk`                                  |
| `CLOTHO_AUTH_REQUIRED`                                        | `true` to require Bearer (managed default; local default `false`) |
| `CLOTHO_TOKEN`                                                | SDK, CLI (`--token`), web server bootstrap path                   |
| `CLOTHO_BOOTSTRAP_TOKEN`                                      | Secret-managed bootstrap/recovery token; never logged             |
| `CLERK_PUBLISHABLE_KEY` / `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` | Web Clerk UI (managed only)                                       |
| `CLERK_SECRET_KEY`                                            | Gateway Clerk Backend / verification (managed only)               |
| `CLERK_JWKS_URL`                                              | Clerk JWKS for session JWT verification                           |
| `CLOTHO_CLERK_JWT_SECRET`                                     | Dev/test HS256 secret (mocks; not for production)                 |

The default local profile uses open bootstrap auth and does not auto-mint a
random credential. Mint a token deliberately via `POST /api/v1/tokens` or
`clotho auth token create`; the plaintext is returned only to that explicit
request and is never written to service logs. Before enabling
`CLOTHO_AUTH_REQUIRED=true`, provision `CLOTHO_BOOTSTRAP_TOKEN` through the
deployment secret manager (or configure the managed AuthProvider). Inspect the
current actor with `GET /api/v1/me`.

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
  -H 'Idempotency-Key: evaluate-v1-01' \
  -d '{"workflow":"evaluate","release_version":"v1.0.0"}' | jq
```

The sandbox receives `CLOTHO_WORKFLOW`, `CLOTHO_COMMIT_ID`,
`CLOTHO_RELEASE_VERSION`, and `CLOTHO_RELEASE_MANIFEST_SHA256`, then executes
`.clotho/evaluate.sh`, `.clotho/inference.sh`, or `.clotho/benchmark.sh`.
It also receives `CLOTHO_RELEASE_PATH` (the fully materialized checkout),
`CLOTHO_RELEASE_URI`, `CLOTHO_REPO_ID`, `CLOTHO_REPO_KIND`, and
`CLOTHO_RELEASE_METADATA` (a generated JSON provenance manifest). Model and
dataset runs enable the relevant framework offline guards automatically, so
`AutoModel.from_pretrained(os.environ["CLOTHO_RELEASE_PATH"],
local_files_only=True)` cannot silently drift to a hosted Hub revision. No user
environment configuration is required.
Queued/running Actions use 30-second database leases and ten-second worker
heartbeats. Expired runs are reclaimed by any healthy gateway replica; the
`attempt` counter records recovery and stale workers cannot overwrite results.

Consumers resolve exact release files from Clotho—not Forgejo or the source
Hub. `HEAD` returns logical size, SHA-256 ETag, commit, manifest digest, and
Arachne hash. `GET` streams large payloads directly from Arachne:

```bash
curl -I http://localhost:8080/api/v1/repos/tiny-gpt/releases/v1.0.0/resolve/model.safetensors
curl -o model.safetensors http://localhost:8080/api/v1/repos/tiny-gpt/releases/v1.0.0/resolve/model.safetensors
# Resume or shard a large transfer without reconstructing unrelated segments.
curl -H 'Range: bytes=1048576-2097151' -o model.part \
  http://localhost:8080/api/v1/repos/tiny-gpt/releases/v1.0.0/resolve/model.safetensors
```

### Hugging Face client compatibility

Immutable releases are projected through Hugging Face's standard read API:

```python
from huggingface_hub import HfApi

hub = HfApi(endpoint="http://localhost:8080", token="clotho_...")
models = list(hub.list_models(search="text-generation", limit=20))
datasets = list(hub.list_datasets(search="training", limit=20))
info = hub.model_info("clotho/tiny-gpt", revision="v1.0.0", files_metadata=True)
files = hub.list_repo_files("clotho/tiny-gpt", revision="v1.0.0")
refs = hub.list_repo_refs("clotho/tiny-gpt")
commits = hub.list_repo_commits("clotho/tiny-gpt", revision="v1.0.0")
```

`main` resolves to the newest immutable release; explicit Clotho versions and
release commit IDs are supported. Model/dataset info, tree traversal, and
`resolve`/`HEAD` downloads come from the frozen manifest and Arachne. The
standard refs API maps `main` to the newest verified release and exposes every
verified immutable Clotho version as a tag. The
commit API walks Clotho VCS history from the release-bound commit, preserving
author, timestamp, title, and message without exposing Forgejo. The
compatibility layer is deliberately read-only—writes still go through Clotho's
audited commit, import, release, and Actions control plane.
Structured JSON files under evaluation/metric/benchmark paths (up to 256 KiB)
are embedded with their source path in the immutable semantic manifest. Hub
metadata exposes the evaluation count, and Clotho's repository page renders
task, dataset, hardware, and metric evidence against the release digest.
Public verified releases support anonymous discovery and downloads even in a
managed profile with authentication required. Private/internal repositories
still require a valid Clotho/Clerk credential and explicit read access; an
invalid supplied credential is never ignored just because a repo is public.

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
  -H 'Idempotency-Key: docs-action-01' \
  -H "Authorization: Bearer $CLOTHO_TOKEN" \
  -d '{"actor":"docs"}' | jq

# list providers (honest configured flags)
curl -s http://localhost:8080/api/v1/providers | jq
```

## Surface map

| Area                | Prefix                                                                                            |
| ------------------- | ------------------------------------------------------------------------------------------------- |
| Auth                | `/api/v1/me`, `/api/v1/tokens`                                                                    |
| Control plane       | `/api/v1/users`, `/orgs`, `/activity`                                                             |
| Repos / VCS         | `/api/v1/repos`, `…/tree`, `…/artifacts`, `…/file`, `…/commits`, `…/submit`                       |
| Repo settings       | `PATCH /api/v1/repos/{name}`, `DELETE /api/v1/repos/{name}`                                       |
| Issues              | `…/issues`, `…/issues/{n}` (PATCH), `…/issues/{n}/comments`                                       |
| Labels / milestones | `…/labels`, `…/milestones` (Slice D)                                                              |
| Notifications       | `/api/v1/notifications`, `…/mark-read` (Slice D)                                                  |
| Pulls               | `…/pulls`, comments, reviews, merge, diff                                                         |
| Merge policy        | `GET/PUT …/merge-policy` (Slice E, ADR-0017)                                                      |
| Actions             | `…/actions/runs`, `…/logs`, `…/config`                                                            |
| Providers           | `/api/v1/providers`, `…/connect` (POST connect / DELETE disconnect), repo `…/imports/huggingface` |
| Secrets             | `/api/v1/orgs/{org}/secrets`, `/api/v1/repos/{repo}/secrets`                                      |
| Agents (presence)   | `…/agent-sessions`                                                                                |
| Agents (admin)      | `/api/v1/agents`, `…/tokens`, `…/audit`                                                           |

### Secret metadata authorization

Secret values are write-only. List and detail responses contain metadata such
as name, scope, description, and optional last-four mask; they never contain a
value or ciphertext. `GET /api/v1/orgs/{org}/secrets[/…]` requires an
administrator of that organization. `GET /api/v1/repos/{repo}/secrets[/…]`
requires a repository administrator or an administrator of its owning
organization.

Clotho authenticates and authorizes these reads before looking up a secret
name. Supplying an invalid credential always returns `401`, including in local
open-auth mode; it never falls back to the bootstrap human. A caller without
the required admin role receives the same `403 permission_denied` response for
an existing or absent secret name, preventing metadata enumeration.

Scoped agents may list repository secret metadata only when their bearer has
the exact repository and `list_secrets` tool scopes. Organization secret lists
remain human-org-admin only. The REST edge revalidates the original agent
bearer; neither surface returns plaintext or ciphertext.

### Human tokens

| Method   | Path                  | Notes                        |
| -------- | --------------------- | ---------------------------- |
| `GET`    | `/api/v1/me`          | Current user + permissions   |
| `GET`    | `/api/v1/tokens`      | Metadata only (prefix, name) |
| `POST`   | `/api/v1/tokens`      | Mint; plaintext `token` once |
| `DELETE` | `/api/v1/tokens/{id}` | Revoke                       |

### Agents admin (Slice C)

Requires human Bearer auth. Caller must be bootstrap user or **org admin**.
The api-gateway proxies to agent-gateway with `CLOTHO_AGENT_ADMIN_TOKEN`; if
that env is unset, admin routes return **503** (`agent management is not
configured`).

| Method   | Path                                      | Notes                                |
| -------- | ----------------------------------------- | ------------------------------------ |
| `GET`    | `/api/v1/agents`                          | List identities                      |
| `POST`   | `/api/v1/agents`                          | `{ name, description? }`             |
| `GET`    | `/api/v1/agents/{name}`                   | Detail + token metadata              |
| `POST`   | `/api/v1/agents/{name}/tokens`            | Mint; plaintext `token` once         |
| `GET`    | `/api/v1/agents/{name}/tokens`            | Metadata only                        |
| `DELETE` | `/api/v1/agents/{name}/tokens/{token_id}` | Revoke                               |
| `PATCH`  | `/api/v1/agents/{name}/tokens/{token_id}` | `{ allowed_repos?, allowed_tools? }` |
| `GET`    | `/api/v1/agents/{name}/audit`             | `{ entries: [...] }`                 |

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

| Method     | Path                              | Notes                        |
| ---------- | --------------------------------- | ---------------------------- |
| `GET/POST` | `/api/v1/repos/{name}/labels`     | List / create label          |
| `GET/POST` | `/api/v1/repos/{name}/milestones` | List / create milestone      |
| `GET`      | `/api/v1/notifications`           | `?unread=true` optional      |
| `POST`     | `/api/v1/notifications/mark-read` | `{ ids: [...] }` or mark all |

Issue and PR create/update bodies accept `labels`, `assignees`, and `milestone`
where applicable.

### Merge policy (Slice E)

| Method | Path                                | Notes         |
| ------ | ----------------------------------- | ------------- |
| `GET`  | `/api/v1/repos/{name}/merge-policy` | Current gates |
| `PUT`  | `/api/v1/repos/{name}/merge-policy` | Update gates  |

`POST …/pulls/{n}/merge` returns **409** with `code: conflict` (or the more
specific `policy_conflict` as individual gates adopt it) when policy blocks
merge, plus the request id for support correlation.

### Internal Forgejo webhook

`POST /api/v1/webhooks/forgejo` is an internal provider boundary, not a public
product API. It always requires `CLOTHO_WEBHOOK_SECRET`, an HMAC-SHA256 over
the exact body, a Forgejo/Gitea event header, and a 1–128 byte visible-ASCII
`X-Forgejo-Delivery` or `X-Gitea-Delivery` id. An empty signing secret,
invalid/missing signature or event,
missing Postgres control plane, missing/ambiguous repository, or malformed
delivery fails closed before CI is scheduled.

Clotho hashes the delivery id and exact body before atomically reserving the
delivery for 24 hours. The first request returns `202 accepted` and schedules
one CI run. An exact concurrent or later retry returns `200 replayed` without
scheduling again. Reusing an id with different request bytes returns
`409 conflict`. Expired rows are removed in bounded batches; plaintext ids,
payloads, signatures, and signing secrets are neither persisted nor logged by
the replay layer.

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
const run = await clotho.createActionRun("weave", {
  actor: "script",
  idempotencyKey: "script-action-01",
});
const logs = await clotho.actionLogs("weave", run.id);
```

## Versioning

- `/api/v1` is the stable-path candidate. During `0.x`, release notes must call
  out any behavioral or schema incompatibility; clients should pin a release.
- Error envelope version `1`, request correlation, repository/activity
  pagination, manual Action-run idempotency, and CLI error classes are frozen.
  The remaining public-alpha gate covers cursor pagination for other unbounded
  collections, idempotency for other retryable mutations, conditional-write,
  asynchronous-operation, cancellation, rate/size-limit, and
  request/audit-correlation conventions.
- Additive changes remain compatible. A removal or semantic change after the
  beta freeze requires a deprecation window, migration note, and versioned path.
- Document a route and its complete request, success, and error schemas in
  OpenAPI **in the same change**. CI structurally verifies
  OpenAPI↔implementation↔SDK coverage and schema shape.
- Internal gRPC, Forgejo routes, database schemas, and provider bridge APIs are
  not public contracts unless a document explicitly promotes them.
