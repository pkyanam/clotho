# Stage 18 — Execution plan: Arachne on the VCS path + BYO object store

**Status:** In progress — pointer/commit/read path and StorageSDK bridge shipped
**Date:** 2026-07-10  
**Owner:** Clotho core  
**PRD:** [docs/prd.md](../prd.md) §5 Stage 18, §6 PRD v3 success criteria  
**ADRs:** [0020](../adr/0020-arachne-vcs-path.md), [0019](../adr/0019-provider-fabric.md) (storage layer), [0014](../adr/0014-secrets-storage-and-resolution.md), [0002](../adr/0002-storage-engine-xet-core.md)

This document is the **handoff spec** for implementing Stage 18. Read the ADRs above before coding. Do **not** start Stage 19 (Tailscale), Stage 20 (durable queue / sandboxes), or Stage 16 (discovery).

---

## North star (one sentence)

Clotho is the open forge where humans and agents ship code and models on the same platform — pluggable compute, storage, and networking — without locking anyone into a cloud.

**Failure mode to avoid:** agents fight the merge queue and compute feels bolted on. Stage 18 is storage/VCS only.

---

## What Stage 17 already shipped (do not redo)

- AuthProvider boundary: `bootstrap` + `clerk` in `crates/clotho-api-gateway/src/auth_provider/`
- Clerk = humans/orgs only; agents stay on `agents` / `agent_tokens` / audit (ADR-0005)
- Provider Fabric **skeleton**: `GET /api/v1/providers?layer=compute|storage|network|auth` (+ `?all=true`)
- Storage/network were **honest stubs** in `crates/clotho-api-gateway/src/providers.rs` (`storage_stubs()`, `network_stubs()`)
- Compute connect/disconnect via secrets in `crates/clotho-api-gateway/src/secrets.rs`
- OpenAPI + sdk-js + CLI + tests (`auth_slice_a`, `auth_clerk`, openapi drift)

---

## Acceptance criteria (ship this, then stop)

1. **ObjectStoreProvider:** org-scoped BYO S3/R2/GCS-compatible bucket via secrets; MinIO remains managed default; `configured` = probe read/write succeeds.
2. **VCS path:** files above threshold (default **10 MiB**, overridable per repo) are chunked via Arachne; git tree stores **git-LFS pointer** blobs; internal metadata may carry Arachne `file_hash`.
3. **Dedup:** identical / near-duplicate large uploads show Stage-2-class chunk dedup against the org's object store (measured, not assumed).
4. **Fetch:** clone/fetch/export/get-file reconstruct byte-identical files from pointers via Arachne.
5. **Repo kind:** `code` | `model` | `dataset` (default `code`) with kind-tuned large-file policy; same forge chrome for all kinds.
6. **Storage UI/API:** Arachne metrics (bytes stored, dedup ratio, chunk counts) — not only VCS tree footprint; honest BYO configured state.
7. **Parity:** REST + OpenAPI + `@clotho/sdk-js` + CLI (+ MCP read/list stubs if needed) in the **same change set**. MCP **never** returns raw object-store credentials.
8. **Fabric:** replace storage stub with real connect/disconnect metadata; **leave network as Stage 17 stub**.
9. **Tests:** measured dedup round-trip through **normal commit API** (env-gated live MinIO); provider configured honesty; bootstrap auth path still green.
10. **Demo:** `just demo` / local stack works **without** BYO S3 (managed MinIO default).

---

## Locked open decisions

### §11 #8 — CI large-file materialization

**Default (implement this):** `ExportRepoArchive` **materializes** LFS/Arachne pointers into real blob bytes inside the exported tarball before CI ships it to sandboxes.

- Rationale: keeps Stage 7 demo honest without inventing an LFS transfer agent or Clotho credential injection into Daytona sandboxes on day one.
- Implementation: gateway `ci.rs` export path resolves pointers via storage gRPC before tar is sent; OR extend `clotho-vcs` export with an optional `resolve_large_files` flag (prefer gateway-side resolution to avoid VCS→storage coupling in the engine if possible).
- Document in an **ADR-0020 amendment** section at implementation time.

### §11 #7 — Human API keys under Clerk

Already resolved in Stage 17: keep minting `clotho_tok_…`; under clerk also accept Clerk session JWTs. **Do not change.**

### git-LFS Batch API

**Out of scope for Stage 18.** Ship pointer format + native Clotho clients (REST/CLI/SDK). Batch API can follow.

---

## Architecture (target)

```text
commit (REST/CLI/MCP)
  → api-gateway: threshold check + kind policy
  → clotho-storage UploadFile (org-scoped bucket)
  → write git-LFS pointer blob via clotho-vcs Commit
  → jj/git tree holds pointer; xorbs/chunks in org object store

read (tree/file/export)
  → detect pointer (version https://git-lfs.github.com/spec/v1)
  → clotho-storage DownloadFile(file_hash from pointer extension or sidecar)
  → return bytes to client / materialize into export tar

BYO storage
  → org connects S3 creds via secrets (ADR-0014)
  → gateway resolves org → StorageConfig → clotho-storage per-request OR dedicated engine instance
  → fabric list shows configured after probe
```

**Hard constraints (repo law):**

- Forgejo stays unmodified and internal (`collab/forgejo`)
- No shelling out to `git`/`jj` in Clotho services
- REST is canonical; web/CLI/SDK/MCP must not drift
- Secrets via ADR-0014 — never return raw secrets
- Do **not** edit `.cursor/plans/`

---

## Current codebase anchors (read before editing)

| Area | Location | Notes |
|---|---|---|
| Fabric storage stub | `crates/clotho-api-gateway/src/providers.rs` | `storage_stubs()` → replace |
| Provider connect | `crates/clotho-api-gateway/src/secrets.rs` | Pattern for daytona/box/computesdk |
| Commit REST path | `crates/clotho-api-gateway/src/lib.rs` | `commit_repo()` → vcs gRPC only today |
| File read | `crates/clotho-api-gateway/src/repos.rs` | `file()` — no pointer resolution |
| CI export | `crates/clotho-api-gateway/src/ci.rs` | `ExportRepoArchive` → ships bare git tar |
| VCS export | `crates/clotho-vcs/src/engine.rs` | `export_repo_archive()` — filesystem tar |
| Arachne engine | `crates/clotho-storage/src/engine.rs` | `StorageConfig`, `ArachneEngine`, dedup |
| Storage gRPC | `proto/clotho/storage/v1/storage.proto` | Upload/Download/GetStorageStats |
| Repo control plane | `crates/clotho-api-gateway/migrations/1002_control_plane.sql` | `repos` table — no `kind` yet |
| Storage web page | `apps/web/app/repos/[name]/storage/page.tsx` | VCS tree only today |
| Stage 2 dedup tests | `crates/clotho-storage/tests/storage.rs` | gRPC-only; reuse patterns |
| Demo large-file upload | `crates/clotho-demo/src/main.rs` | Uses storage gRPC directly (not VCS path) |

**Gap:** `AppState` has no `StorageClient` today. Gateway talks to vcs/diff/queue/compute only. Stage 18 must add `storage_grpc_url` to `GatewayConfig` and a lazy `StorageClient` on `AppState` (mirror other gRPC clients in `lib.rs`).

---

## Implementation slices (execute in order)

### Slice 1 — ObjectStoreProvider + fabric honesty

**Goal:** BYO S3 connect/disconnect; fabric `?layer=storage` shows real configured state; MinIO default when unconnected.

#### 1.1 Migration

**File:** `crates/clotho-api-gateway/migrations/1009_object_store.sql`

```sql
-- Org-scoped object store connection (optional repo override later).
create table if not exists object_store_connections (
    id text primary key,
    org_id text not null references orgs (id) on delete cascade,
    repo_id text references repos (id) on delete cascade,  -- null = org default
    provider_id text not null default 's3',  -- s3 | r2 | gcs | minio
    endpoint text not null,
    region text not null default 'us-east-1',
    bucket text not null,
    path_prefix text not null default '',
    access_key_secret_name text not null,  -- ADR-0014 org secret name
    secret_key_secret_name text not null,
    last_probe_at timestamptz,
    last_probe_ok boolean,
    last_probe_error text not null default '',
    created_by text not null references users (id),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (org_id, repo_id)  -- one connection per org or per repo override
);
```

Use well-known secret names, e.g. `CLOTHO_S3_ACCESS_KEY`, `CLOTHO_S3_SECRET_KEY` (org-scoped), or per-connection generated names — pick one pattern and document in `docs/api.md`.

#### 1.2 New module

**File:** `crates/clotho-api-gateway/src/object_store.rs` (add `mod object_store;` in `lib.rs`)

Responsibilities:

- `load_connection(pool, org_id, repo_id?) -> Option<ObjectStoreConnection>`
- `resolve_storage_config(state, org_id, repo_id?) -> StorageConfig` — BYO row or env MinIO default
- `probe_connection(config) -> ProbeResult` — write/read/delete `clotho-probe/<uuid>` object
- `list_storage_providers(state, org_id) -> Vec<FabricProvider>` — replace `storage_stubs()`
- `connect_object_store(...)` / `disconnect_object_store(...)`

#### 1.3 Extend provider connect

**File:** `crates/clotho-api-gateway/src/secrets.rs`

Extend `connect_provider` / `disconnect_provider` / `provider_secret_names` for storage ids:

- `s3`, `r2`, `gcs` (all S3-compatible via `object_store`)
- `minio` (alias / managed — may only show as default, not BYO connect)

**Connect request body** (extend `ConnectProviderRequest` or add storage-specific fields):

```json
{
  "org": "clotho",
  "endpoint": "https://…",
  "region": "auto",
  "bucket": "my-bucket",
  "path_prefix": "clotho/",
  "access_key": "…",
  "secret_key": "…"
}
```

Seal keys via existing `upsert_org_secret`; store connection row with secret **names** only.

#### 1.4 Replace fabric stub

**File:** `crates/clotho-api-gateway/src/providers.rs`

- Remove hardcoded `configured: false` stub for MinIO
- `fabric_for_layer(Storage)` calls `object_store::list_storage_providers`
- Managed MinIO: `configured: true` when default env bucket probe succeeds (or when compose stack is up)
- BYO row: `configured` from last probe; overlay `connected · ···last4` like compute

#### 1.5 Gateway config

**Files:** `crates/clotho-api-gateway/src/main.rs`, `GatewayConfig`, `AppState`, `docker-compose.dev.yml`, `.env.example`

Add:

- `CLOTHO_STORAGE_GRPC_URL` (default `http://clotho-storage:50052` or whatever port compose uses)
- Default managed store env vars already on `clotho-storage` service — gateway reads same for fallback resolution

#### 1.6 Tests

**File:** `crates/clotho-api-gateway/tests/stage18_storage.rs` (new, env-gated)

- Probe round-trip against MinIO
- `GET /api/v1/providers?layer=storage` shows `configured: true` for managed default
- Connect BYO (can use second prefix in same MinIO bucket for test)
- Disconnect clears configured
- `auth_clerk.rs` storage stub assertion (`configured: false`) **must be updated**

---

### Slice 2 — Pointer contract + gateway↔storage wiring

**Goal:** Shared pointer format; gateway can upload/download via storage gRPC with org-scoped config.

#### 2.1 Pointer format (git-LFS compatible)

Store as the git blob bytes (UTF-8 text):

```text
version https://git-lfs.github.com/spec/v1
oid sha256:<hex-of-raw-file-bytes>
size <original-uncompressed-size>
x-clotho-arachne-hash <merkle-file-hash-hex>
```

- `oid sha256` uses standard LFS semantics (hash of full file content)
- `x-clotho-arachne-hash` is a **Clotho extension line** (ignore unknown lines per LFS spec) — the handle for `DownloadFile`
- Detection: first line equals `version https://git-lfs.github.com/spec/v1`

**File:** `crates/clotho-common/src/lfs_pointer.rs` (or `crates/clotho-api-gateway/src/lfs_pointer.rs` if keeping common minimal)

Functions: `is_lfs_pointer`, `parse_lfs_pointer`, `build_lfs_pointer`, `materialize_pointer`

#### 2.2 Storage service: org-scoped backends (choose one approach)

**Option A (recommended for Stage 18):** gateway passes resolved `StorageConfig` on each upload/download via new gRPC metadata or proto fields.

- Extend `proto/clotho/storage/v1/storage.proto`:
  - `UploadFileRequest` / `DownloadFileRequest` optional `StoreContext { endpoint, bucket, prefix, access_key, secret_key, region }` **or** reference `connection_id`
  - Prefer **not** sending raw keys on every chunk — instead add `ConfigureStore(StoreContext)` unary RPC that returns a session id, OR resolve in gateway and pass only to a new `UploadFileForStore` RPC

**Option B (simpler, acceptable for demo):** single `clotho-storage` process per org bucket is overkill; instead run one storage service but add `SetActiveStore(StoreContext)` admin RPC (not product-safe multi-tenant).

**Pragmatic Stage 18 choice:** extend proto with optional `store_override` on first message of upload stream + download request; `clotho-storage` builds ephemeral `ArachneEngine` for that request when override present, else uses process env default. Document tradeoff in ADR-0020 amendment.

**Files:**

- `proto/clotho/storage/v1/storage.proto`
- `crates/clotho-storage/src/service.rs`
- `crates/clotho-storage/src/engine.rs` — ensure `ArachneEngine::new(config)` is cheap enough or cache by `(endpoint,bucket,prefix)` key
- Regenerate / update `clotho-common` pb types

#### 2.3 Gateway storage client

**File:** `crates/clotho-api-gateway/src/lib.rs`

- Add `storage: StorageClient<Channel>` to `AppState`
- Helper: `storage_for_repo(state, repo_name) -> StorageClient + StoreContext`

---

### Slice 3 — VCS commit/fetch path

**Goal:** Normal commit API routes large files through Arachne; reads resolve pointers.

#### 3.1 Threshold + kind policy

**File:** `crates/clotho-api-gateway/migrations/1008_repo_kind.sql`

```sql
alter table repos add column if not exists kind text not null default 'code'
    check (kind in ('code', 'model', 'dataset'));
alter table repos add column if not exists large_file_threshold_bytes bigint;
-- null = use kind default (10 MiB code; 0 or 1 MiB for model/dataset — pick sensible defaults)
alter table repos add column if not exists storage_attrs jsonb not null default '{}'::jsonb;
```

Kind defaults (in Rust, not DB):

| Kind | Default threshold | `storage_attrs` hints |
|---|---|---|
| `code` | 10 MiB | threshold only |
| `model` | 1 MiB | extensions: safetensors, gguf, onnx, bin, pt, … |
| `dataset` | 1 MiB | extensions: parquet, arrow, csv.gz, … |

Extension match → force Arachne even below threshold.

#### 3.2 Commit API: binary/large file input

Current REST commit uses `content: string` (UTF-8 text only). **Extend:**

```json
{
  "path": "weights/model.safetensors",
  "content_base64": "<base64>",
  "executable": false
}
```

- Exactly one of `content` (text) or `content_base64` (binary)
- OpenAPI + SDK + CLI updated together

**File:** `crates/clotho-api-gateway/src/lib.rs` — `commit_repo()`

Pseudocode:

```rust
for each file:
  bytes = decode(content | content_base64)
  if should_offload_to_arachne(repo_kind, path, bytes.len()):
    upload = storage.upload_file(stream bytes, store_context)
    blob = build_lfs_pointer(upload)
  else:
    blob = bytes
  vcs.commit(files with blob bytes)
```

#### 3.3 Read path

**File:** `crates/clotho-api-gateway/src/repos.rs`

- `file()`: if blob is LFS pointer → download via storage → return bytes; set `binary: true`, `content: null`, add optional `content_base64` or `download_url` (pick one — **recommend `content_base64` for parity with commit**, document size limits)
- `tree()` / `ListFiles`: for pointer blobs, report **logical size** from pointer `size` line, not pointer text length; add `stored_via: "arachne" | "git"` on tree entries (OpenAPI + SDK)

#### 3.4 Export / CI materialization

**File:** `crates/clotho-api-gateway/src/ci.rs`

After `export_repo_archive`:

1. Walk exported bare repo OR re-walk via vcs `ListFiles` + `GetFile` at exported commit
2. For each pointer path, write materialized blob into a **companion** directory in the tar, OR rewrite git objects (harder)

**Simpler approach aligned with ADR-0008 shipping model:**

- Add `large-files/` directory to tar alongside `repo.git/` with materialized paths
- Patch CI clone script in `crates/clotho-compute` / `ci.rs` to overlay `large-files/` into working tree after clone

**Even simpler (preferred if time-constrained):**

- Before tar, use vcs to build a **full working tree export** at commit (new optional RPC `ExportRepoTree` returning tar of resolved files + `.git` bare) — heavier but one artifact

Pick the smallest approach that makes `just demo` CI see real bytes. Document choice in ADR-0020 amendment.

---

### Slice 4 — Repo kind API + storage stats

#### 4.1 Repo create / PATCH

**Files:**

- `crates/clotho-api-gateway/src/control.rs` — `CreateRepoRequest`, `UpdateRepoRequest`, insert/update SQL
- `crates/clotho-api-gateway/src/lib.rs` — create repo handler
- `crates/clotho-api-gateway/src/repos.rs` — PATCH handler, repo detail JSON

Add to API:

- `POST /api/v1/repos` body: optional `kind`
- `PATCH /api/v1/repos/{name}` body: optional `kind`, `large_file_threshold_bytes`, `storage_attrs`
- Repo detail/list responses include `kind`

#### 4.2 Storage stats endpoint

**Route:** `GET /api/v1/repos/{name}/storage`

**File:** `crates/clotho-api-gateway/src/repos.rs` (or new `storage.rs` module)

Response (metadata only):

```json
{
  "repo": "my-model",
  "kind": "model",
  "object_store": {
    "provider_id": "minio",
    "configured": true,
    "configured_reason": "managed default",
    "bucket": "clotho-storage",
    "endpoint": "http://minio:9000"
  },
  "arachne": {
    "xorb_count": 12,
    "xorb_bytes": 67108864,
    "shard_count": 3,
    "shard_bytes": 4096,
    "total_bytes": 67112960,
    "dedup_ratio": 0.99,
    "chunk_count": 33222
  },
  "vcs_footprint": {
    "files": 42,
    "total_bytes": …
  }
}
```

- `arachne` from storage `GetStorageStats` scoped to org prefix
- `dedup_ratio` derived from repo's pointer files vs arachne totals (approximation OK if documented)
- May need proto extension for prefix-scoped stats OR filter by prefix in gateway from object store listing

#### 4.3 Web storage page

**File:** `apps/web/app/repos/[name]/storage/page.tsx`

Replace "deep object store metrics are not part of this view" with:

- Arachne stat cells (total bytes, dedup ratio, xorbs, chunks)
- Object store configured banner (managed vs BYO)
- Keep VCS tree footprint section below

Optional: org settings storage connect form (mirror compute connect in settings) — if missing, link to `clotho provider connect s3 …` in docs.

---

### Slice 5 — Parity surfaces (same PR)

| Surface | Files | Work |
|---|---|---|
| OpenAPI | `docs/openapi.yaml` | kind, storage stats, content_base64, tree `stored_via`, provider connect fields |
| API docs | `docs/api.md` | examples for storage connect, commit large file, storage stats |
| CLI docs | `docs/cli.md` | remove "stub until Stage 18" notes |
| SDK | `packages/sdk-js/src/index.ts` + tests | `RepoKind`, `getStorageStats`, `content_base64` on commit, provider connect S3 fields |
| CLI | `crates/clotho-cli/src/main.rs` | `clotho provider connect s3`, repo kind on create, storage stats command |
| MCP | `crates/clotho-agent-gateway/src/mcp.rs` | `list_providers?layer=storage` (already proxies REST); optional `get_storage_stats` tool — metadata only |
| Env | `.env.example` | `CLOTHO_STORAGE_GRPC_URL`, `CLOTHO_ARACHNE_THRESHOLD_BYTES` |
| ADR | `docs/adr/0020-arachne-vcs-path.md` | amendment: CI materialization default + proto/store override note |

---

### Slice 6 — Tests (env-gated + unit)

#### 6.1 Gateway integration test (primary acceptance test)

**File:** `crates/clotho-api-gateway/tests/stage18_arachne_vcs.rs`

Env gates (mirror storage tests):

- `CLOTHO_GATEWAY_TEST_URL` or spin gateway in test
- `CLOTHO_STORAGE_TEST_S3_ENDPOINT`
- `CLOTHO_SECRETS_MASTER_KEY` for connect path

Scenario:

1. Create repo `kind=model`
2. `POST /commits` with `content_base64` — synthetic 64 MiB file (or `CLOTHO_STORAGE_TEST_FILE_MB`)
3. Record storage stats / object store bytes
4. Second commit with near-duplicate (64 KiB patch + 1 KiB insert — reuse pattern from `storage.rs`)
5. Assert growth ≪ file size (Stage 2 class)
6. `GET /file` returns byte-identical content
7. `GET /storage` shows arachne metrics with `configured: true`

#### 6.2 Keep green

- `auth_slice_a`, `auth_clerk` (update storage expectations)
- `openapi_drift`
- `packages/sdk-js` vitest
- Plain `cargo test` skips env-gated tests

#### 6.3 justfile

Add `just test-stage18` if helpful (compose up + env vars + single test binary).

---

## REST API summary (new/changed)

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/providers?layer=storage` | Real providers, not stub |
| POST | `/api/v1/providers/{s3\|r2\|gcs}/connect` | Extended body with endpoint/bucket/keys |
| DELETE | `/api/v1/providers/{id}/connect?org=` | Clears BYO connection + secrets |
| POST | `/api/v1/repos` | Optional `kind` |
| PATCH | `/api/v1/repos/{name}` | Optional `kind`, threshold, attrs |
| POST | `/api/v1/repos/{name}/commits` | `content_base64` for binary; auto Arachne |
| GET | `/api/v1/repos/{name}/file` | Resolves pointers; optional `content_base64` out |
| GET | `/api/v1/repos/{name}/tree` | Logical sizes + `stored_via` |
| GET | `/api/v1/repos/{name}/storage` | Arachne + object store honesty |

---

## Verification checklist (before declaring Stage 18 done)

- [ ] `GET /api/v1/providers?layer=storage` — MinIO managed shows configured in dev compose
- [ ] BYO connect + probe + disconnect round-trip
- [ ] Commit 64 MiB+ file through REST → tree shows pointer-sized entry with logical size
- [ ] Near-duplicate second commit → measured dedup (object store bytes)
- [ ] File download byte-identical to upload
- [ ] Export/CI path includes materialized bytes (demo CI or unit test)
- [ ] Repo kinds create/PATCH/list/detail
- [ ] Web storage page shows Arachne metrics
- [ ] OpenAPI/SDK/CLI/MCP updated; no credential leakage
- [ ] `just demo` works without BYO bucket
- [ ] Network fabric still stub; no Tailscale code
- [ ] ADR-0020 amended with CI materialization decision

---

## Explicitly out of scope (Stage 19+)

- Tailscale NetworkProvider, `clotho-runner`, private-net jobs (ADR-0021)
- Durable merge-queue, `/sandboxes`, provenance trailers (ADR-0022)
- Stage 16 discovery / signals / leaderboards
- git-LFS Batch API / `git lfs` transfer agent
- Cross-org dedup marketplace
- Forgejo source changes

---

## Handoff to Stage 19 (after Stage 18 ships)

Summarize for the user:

- **Stage 19:** Tailscale connect, ephemeral tagged CI nodes, `clotho-runner` BYOC compute provider, `private-net` capability gating in fabric scheduling
- **Stage 20:** Postgres-backed merge-queue, speculative CI, `/sandboxes` API, provenance trailers
- Storage and VCS pointer work should **not** block Tailscale — network stub stays until 19

---

## Suggested commit breakdown (if user asks to commit)

1. `feat(gateway): ObjectStoreProvider connect + fabric storage layer`
2. `feat(storage): per-request store override + pointer helpers`
3. `feat(vcs-path): Arachne offload on commit + pointer resolution on read`
4. `feat(repos): kind column + storage stats API`
5. `feat(web,sdk,cli): Stage 18 parity surfaces`
6. `test(gateway): stage18 measured dedup via commit API`
7. `docs: ADR-0020 amendment + Stage 18 execution note`

Do **not** commit unless the user asks.

---

*Living doc: update checkboxes and implementation notes as slices land; do not silently expand scope into Stage 19.*
