# ADR-0008: The CCI is a Rust-native trait; Daytona is the first provider; git objects ship into the sandbox

- **Status:** Accepted
- **Date:** 2026-07-07
- **Deciders:** Clotho core

## Context

Stage 7 (docs/prd.md §5, the final prototype stage) wires one real external
compute provider end-to-end: a push to a repo triggers a CI job that runs on
an external sandbox and reports status back to the PR. The vision spec (§4.1)
names the abstraction — the **Clotho Compute Interface (CCI)** — and says to
either adopt ComputeSDK or "define a compatible interface so anything that
speaks ComputeSDK's provider model works with minimal glue." No-lock-in is a
core product stance (§1), so the provider must sit behind a swappable
interface, not be hardcoded.

Three sub-decisions needed a call:

1. **Provider** — PRD §8 open decision #3: Daytona (persistent workspaces,
   fast cold start) vs E2B (microVM isolation). _Human decision:_ Daytona,
   kept modular behind the CCI.
2. **How the CCI talks to the provider.** ComputeSDK and Daytona's own SDKs
   are TypeScript/Python/Ruby/Go — there is **no Rust SDK**. Options: call a
   small TS worker wrapping ComputeSDK, or implement the provider's REST API
   directly from Rust.
3. **How the pushed commit's code reaches the sandbox.** Daytona sandboxes
   run in Daytona's cloud; they **cannot reach the local `docker compose`
   Forgejo/gateway by service name**. So the naive "sandbox fetches over
   git-http from the stack" is not reachable without exposing the local stack
   publicly (a tunnel token — not reproducible from a clean `docker compose
up`, which the exit condition requires).

## Decision

**The CCI is a Rust-native trait, and Daytona is implemented directly against
its REST API — no TS runtime in the loop.**

- New crate `clotho-compute` defines `ComputeProvider` (the CCI): a thin
  async trait — `run_job(JobSpec) -> JobResult` — over generic sandbox
  primitives (create from snapshot → place files → run commands → collect
  exit code + logs → tear down). `DaytonaProvider` implements it with
  `reqwest`: control plane at `https://app.daytona.io/api` (create/get/delete
  sandbox) and the toolbox proxy at `https://proxy.app.daytona.io/toolbox/{id}`
  (`/process/execute`, `/files/upload`), both Bearer-authenticated with
  `DAYTONA_API_KEY`. The crate exposes a gRPC `Compute` service on :50057 like
  every other backend service.
- **Why Rust-native over a TS worker:** the abstraction we must own is the
  CCI trait itself (no-lock-in), so adopting ComputeSDK-the-library would add
  a second abstraction layer — and a Node runtime/container/deploy unit — on
  top of ours. A second provider (E2B) is another impl of the same trait, not
  a rewrite. The surface Stage 7 needs (create / exec / upload / status /
  teardown) is small enough that owning it in Rust keeps the backend
  all-Rust/gRPC (PRD §2) without meaningful cost. The trade-off we accept: the
  official SDKs' retries, toolbox-routing quirks, and log streaming are ours
  to reimplement as needed; we keep the Daytona calls narrowly scoped to what
  the demo needs rather than reproducing the whole SDK.
- **Compute stays both vendor-agnostic and collaboration-agnostic.** It knows
  nothing about Forgejo or git; it runs commands in a sandbox. CI orchestration
  (build the check script, report status) lives in the api-gateway, where the
  Forgejo coupling (ADR-0003's GPLv3 API boundary) already lives.

**Git objects are shipped into the sandbox, not fetched by it.** clotho-vcs
gains an `ExportRepoArchive(repo)` RPC that returns a tar of the repo's
backing **bare git repository** — the real git object database the engine
writes (never a `jj`/`git` shell-out; a filesystem tar via the `tar` crate).
The api-gateway hands that archive to `clotho-compute` as a job file; the CI
script inside the sandbox untars it, `git clone`s the bare repo, checks out
the pushed commit, and runs the check. This is genuinely "fetch the repo's
git objects and build/test them" — the objects are just _delivered_ as an
archive instead of pulled over git-http, because the cloud sandbox has no
route back to the local stack. It keeps the exit condition's "reproducible
from a clean `docker compose up`" honest (only a `DAYTONA_API_KEY` in `.env`
is needed) and stays provider-agnostic (E2B would deliver the same way).

**The CI check** (chosen with the human): the script runs a repo-defined
`.clotho/ci.sh` if present, else a sensible default probe (Makefile → `make`;
Cargo → `cargo test`; package.json → the `test` script), and reports the
check's exit code as the commit status.

**Credentials:** `DAYTONA_API_KEY` (plus optional `DAYTONA_API_URL`,
`DAYTONA_TARGET`, `CLOTHO_COMPUTE_SNAPSHOT`) come from a gitignored `.env`
only. With no key set, `clotho-compute` starts in a `disabled` mode: the
gRPC surface stays up but `RunJob` returns `FAILED_PRECONDITION`, and the
env-gated integration test self-skips — so plain `cargo test` and CI stay
green without a paid credential (matching the storage/collab/agent tests).

## Consequences

- A push webhook from Forgejo → api-gateway → `clotho-compute` → a real
  Daytona sandbox → commit status back on the PR, all behind one swappable
  trait. Adding E2B later is a new `ComputeProvider` impl and one env var.
- The api-gateway registers a per-repo push webhook at repo-creation time
  (Forgejo → `http://clotho-api-gateway:8080/api/v1/webhooks/forgejo`),
  guarded by a shared `CLOTHO_WEBHOOK_SECRET` (HMAC-SHA256 over the body,
  Forgejo's `X-Gitea-Signature`). The receiver fails closed when that secret
  or Postgres is absent, requires a bounded Forgejo/Gitea delivery id, resolves
  one unambiguous Clotho repository, and atomically persists only SHA-256
  hashes of the id and exact payload before scheduling. The first delivery
  returns `202`; an exact replay returns a harmless `200` without a second CI
  task; changed bytes under one id return `409`. Reservations expire after 24
  hours and cleanup is batch-bounded. The background task posts `pending` →
  runs the job → posts `success`/`failure`.
- `ExportRepoArchive` returns the whole bare-repo object DB (all refs), not a
  single-commit bundle — fine at prototype scale; a bundle of just the pushed
  revision is the obvious optimization before large repos.
- Because the sandbox runs in Daytona's cloud, a self-hosted Daytona runner
  ("Bring Your Own Compute") on the dev network — which _would_ let the
  sandbox fetch over git-http — is a documented later option, not wired here.
- Live end-to-end verification requires a real `DAYTONA_API_KEY`; the demo
  script (`scripts/demo`) drives the whole path and self-skips the compute
  leg with a clear message when the key is absent.
