# ADR-0020: Arachne on the VCS path — large files, LFS bridge, repo kinds

- **Status:** Accepted
- **Date:** 2026-07-09
- **Deciders:** Clotho core
- **Related:** ADR-0002 (xet-core), ADR-0019 (ObjectStoreProvider), vision §3.2

## Context

`clotho-storage` (Arachne) already delivers measured chunk-level dedup against
S3-compatible backends (Stage 2 exit condition). It is **not** on the normal
commit/clone/push path: `clotho-vcs` writes git objects only; the web Storage
page shows VCS trees, not Arachne metrics; there is no git-LFS / Xet bridge.

Hugging Face Hub proved that model/dataset hosting needs chunk-level storage
(Xet) with an LFS-compatible edge so existing clients keep working. Clotho's
Dream Roadmap makes "host repos of models too" a first-class goal. Until
VCS↔Arachne is wired, Arachne remains a demo island and Clotho cannot displace
GitHub+LFS or HF for large artifacts.

## Decision

### 1. Wire Arachne into the commit / fetch path

For files above a configurable threshold (default aligned with common LFS
practice, e.g. 10 MiB, overridable per repo):

1. **Commit (API / CLI / MCP):** content is chunked via Arachne; the git/jj
   tree stores a **pointer file** (git-LFS pointer format at the edge for
   compatibility; internal metadata may carry Xet/Arachne hashes).
2. **Fetch / clone / export:** pointer resolution reconstructs bytes from
   xorbs/chunks in the org's ObjectStoreProvider (ADR-0019).
3. **Identical / near-duplicate uploads** dedup at chunk level across the
   org's store (permission-aware: only reconstruct chunks the caller may
   read — follow Xet's permission model as closely as the open protocol
   allows; start with org-scoped buckets if chunk-ACL is deferred).

Small text/code files continue to live as ordinary git blobs — no forced
detour through Arachne.

### 2. LFS bridge at the edges

- Speak **git-LFS Batch API** (and/or custom transfer agent semantics) so
  `git lfs` and HF-style clients can push/pull large files during migration.
- Prefer native Clotho clients (CLI/SDK/web upload) that talk Arachne
  directly for best throughput.
- Import path: existing LFS repos can be adopted; pointers remain valid;
  background migration to full Arachne layout is allowed without locking
  the repo (same spirit as HF's LFS→Xet migration).

### 3. Repo kinds: `code` | `model` | `dataset`

Repos gain a `kind` field (default `code`):

| Kind | Default large-file policy | UI emphasis |
|---|---|---|
| `code` | Threshold-based Arachne | Source browser, PRs, Actions |
| `model` | Aggressive binary tracking (safetensors, gguf, onnx, … via attributes) | File sizes, dedup savings, model card |
| `dataset` | Same as model + parquet/arrow-friendly attrs | Sample preview hooks later |

Same forge chrome (issues/PRs/agents) for all kinds. Kind is not a separate
product — it tunes storage defaults and overview UI.

### 4. Product surfaces

- **Storage settings / page:** show Arachne metrics (bytes stored, dedup
  ratio, chunk counts) for the repo/org — not only the VCS tree.
- **REST:** repo create/PATCH accepts `kind`; storage stats endpoints under
  `/api/v1/repos/{name}/storage` (metadata only).
- **CLI/MCP/SDK:** parity for kind + storage stats; MCP never returns raw
  object-store credentials.
- **Performance:** publish clone/push benchmarks vs git+LFS and HF Hub on
  equivalent workloads (vision §5) once the path is real — not before.

### 5. Non-goals (this ADR)

- Global cross-org dedup marketplace.
- Full HF Spaces / interactive app hosting.
- Replacing git object storage entirely with buckets (Trunks-style) — Clotho
  keeps jj/git commits as the source of truth for history; Arachne holds
  large blob payloads.

## Consequences

- `clotho-vcs` and `clotho-storage` must share a clear pointer contract
  (protobuf or shared crate types) — first real cross-engine product
  dependency beyond "demo uploads."
- ExportRepoArchive / CI shipping must either include resolved large files
  or teach sandboxes to fetch pointers via Clotho credentials — design at
  implementation time; do not silently ship empty pointers into CI.
- ObjectStoreProvider (ADR-0019) should land before or with the first
  customer BYO bucket; MinIO remains the default for `just demo`.
- Acceptance: upload a multi-GB model twice (second with small delta) through
  normal commit API; storage growth matches Stage 2-class dedup; clone via
  documented client reconstructs byte-identical files.
