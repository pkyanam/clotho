# Clotho — Product Requirements
### Prototype record, modular-platform roadmap, and public-release plan
**v0.5 — July 2026**

---

## 0. Purpose of this document

This PRD started as the scope for a **working internal prototype**. Stages 0-10 record the path from scaffold to jj-backed VCS, Arachne storage, Forgejo facade, MCP agents, CLI, Actions, Daytona compute, and a basic web shell.

PRD v2 shifts the product from "prove the stack works" to "make Clotho a mature modular platform." The default direction is:

- Keep Forgejo internal, unmodified, and behind Clotho-owned API/web surfaces.
- Keep the Rust-native CCI as Clotho's compute contract.
- Add ComputeSDK as an optional bridge/adapter behind CCI, not as the core product boundary.
- Add Box later as a richer persistent VM / agent-workspace provider, not as a replacement for short-lived CI sandboxes.
- Treat web, REST, SDK, CLI, and MCP parity as part of feature maturity, not separate polish.

**PRD v3 (Dream Roadmap)** turns the vision's idealism into sequenced product work: Clerk human auth, a Provider Fabric (compute + BYO storage + Tailscale network), Arachne on the real VCS path (code *and* model/dataset repos), and an agent runtime that does not feel bolted on (durable merge-queue, sandboxes, provenance). See §5 Stages 17–21 and ADRs 0018–0022. Discovery (Stage 16) stays after those foundations.

**PRD v4 makes public trust the immediate next milestone.** Stage 22 is the
release gate even where earlier capability stages have remaining work: API,
CLI, SDK, MCP, security, durability, packaging, accessibility, and an
unfamiliar-agent handoff must be hardened before Clotho makes a stable platform
claim. Stage 23 establishes production identity and tenant isolation; Stage 24
establishes the hosted/self-hosted production control plane and elastic workload
fleet. Stages 25–29 then sequence the most defensible frontier ideas. See
[`release-readiness.md`](release-readiness.md) and
[`frontier-roadmap.md`](frontier-roadmap.md).

**PRD v5 makes the hosted and self-hosted ambitions one production
architecture.** Stages 23–24 establish tenant-safe multi-user identity,
authorization, supported deployment profiles, durable scheduling, quotas,
metering, HA operations, and elastic workload cells before frontier surface
growth. The former Stages 23–27 move to Stages 25–29. Signals are the approved
lightweight public-interest primitive and remain distinct from Following,
authority, and Lachesis-derived adoption (ADRs 0023–0024).

---

## 1. Prototype scope — what "working" means

**In scope for the prototype:**
- A real jj-backed VCS engine, exposed as a service (not just the `jj` CLI)
- A real Arachne storage engine implementing chunk-level dedup against an S3-compatible backend
- Forgejo running as the collaboration shell (issues/PRs/web chrome), wired to repos the VCS engine manages
- A minimal MCP-based agent interface: checkpoint, restore, structured diff, orient
- A basic web frontend (evolved from the teaser page) that can browse a repo and a PR
- One real pluggable compute provider wired end-to-end for CI (not all six from the vision doc)

**Explicitly out of scope for the prototype** (real, but deferred):
- ActivityPub federation
- Multi-agent merge-queue at production robustness (a naive version is enough to prove the concept)
- Tailscale/private-cloud networking (stub the interface, don't fully wire it)
- Database-connector framework for external data sources
- Billing, managed hosting, multi-tenant isolation hardening

**Definition of done for the prototype (Stage 7 demo):**
Two simulated agent sessions push concurrent commits to the same repo → the merge-queue reconciles them without a human in the loop → a large binary file uploaded twice (once modified) shows measurable chunk-level dedup in storage → a human reviews the resulting PR in the web UI → a CI job triggered by the push runs on a real external sandbox provider and reports status back.

---

## 2. Tech stack decisions

| Layer | Choice | Why |
|---|---|---|
| VCS engine | **Rust, built on `jj-lib`** | `jj-lib` is explicitly designed to be usable from a server serving requests from multiple users, not just the CLI — it's the right embedding point rather than shelling out to the `jj` binary. Git-compatible storage backend (via `gitoxide`) means every commit Clotho produces is a real git object, so nothing downstream needs to know jj is involved. |
| Storage engine (Arachne) | **Rust, built on `xet-core`** | The Xet protocol is published and implementation-agnostic, and the reference implementation (`xet_pkg`, `xet_client`, `xet_data`, `xet_core_structures`, `xet_runtime`) is Apache/MIT-licensed Rust with existing WASM bindings — we can embed it directly rather than reimplementing GearHash chunking and xorb formats from scratch. |
| Collaboration shell | **Forgejo (Go), run largely as-is, PostgreSQL-backed** | Forgejo is a mature, actively maintained, non-profit-governed Gitea fork with issues, PRs, org/permissions, and a documented Postgres schema. For the prototype, don't attempt deep jj-native UI integration inside Forgejo — point it at the same git-compatible objects the VCS engine writes, and treat Forgejo purely as the collaboration/issue/PR chrome. **Licensing flag:** Forgejo ≥v9.0 is GPLv3. If we fork and modify Forgejo's own source, GPLv3's copyleft applies to that code specifically (running it as SaaS without distributing modified source is fine under GPLv3, unlike AGPL — but any modified Forgejo code we *do* distribute must stay GPLv3). This is why Forgejo is walled off in its own `collab/` directory rather than mixed into Clotho's own codebase — see §5 and the open decision in §11. |
| Agent interface | **MCP server, Rust** (`clotho-agent-gateway`) | Exposes `checkpoint`, `restore_to`, `diff_symbol`, `orient_repo` as MCP tools backed directly by the VCS engine's gRPC API. Note: a third-party crate (`agentic-jujutsu`) already explores this space with WASM bindings and MCP transport — worth reading for interface ideas, but its production-readiness claims are unverified and it's not an org we have visibility into, so we build our own gateway directly on `jj-lib` rather than depending on it for the core engine. |
| Agent identity | Scoped tokens in Postgres, distinct table/model from human OAuth identities, per-action audit log | Matches the vision spec's non-human-identity requirement from day one, even in prototype form — retrofitting identity models later is painful. |
| Structured diff | Rust, `tree-sitter` for symbol-level parsing | Feeds both the human PR view and the agent-facing diff API from one object. |
| Compute abstraction | **Rust-native CCI**, with direct providers and optional bridge providers | ADR-0008 resolved the prototype on a Rust-native `ComputeProvider` trait plus direct Daytona provider. PRD v2 keeps that boundary: ComputeSDK can sit behind CCI as an optional TypeScript bridge for broad provider coverage, routing, failover, and a unified sandbox API, but Clotho services should depend on CCI rather than a specific vendor SDK. |
| Frontend | **Next.js (React) + Tailwind**, reusing the design tokens/components from the teaser page | Ship the app shell as a proper design system (`packages/ui`) rather than one-off marketing CSS, so the teaser page's visual language survives into the product. |
| API gateway | **Rust (Axum) or TypeScript (tRPC)** — recommend Axum to stay in one language for the backend services and share types via generated OpenAPI/protobuf | Aggregates VCS engine, storage engine, agent gateway, and proxies to Forgejo's API. |
| Inter-service protocol | gRPC (protobuf) between Rust services; REST/JSON at the edge for the frontend and Forgejo webhook integration | Keeps internal services fast and strongly typed; keeps the edge boring and debuggable. |
| Dev environment | Docker Compose (Postgres, MinIO as S3-compatible storage, Forgejo, all Clotho services) | Single `docker compose up` for a fresh contributor or agent to get a working stack. |
| Monorepo tooling | **Cargo workspace** for all Rust crates + **pnpm workspace** for all TS/JS, tied together by a root **`justfile`** for cross-language tasks; **Turborepo** for JS build caching | Avoids forcing a single polyglot build tool (Bazel/Nx) on a small early-stage team — `just` is enough glue, and each language keeps its native, well-supported workspace tooling. Revisit Bazel/Nx only if CI times become a real problem. |

---

## 3. Monorepo structure

```
clotho/
├── README.md
├── LICENSE                        # top-level Clotho license (decision needed — see §11)
├── justfile                       # cross-language task runner: `just dev`, `just test`, `just build`
├── docker-compose.dev.yml
├── Cargo.toml                     # Rust workspace root
├── pnpm-workspace.yaml
├── turbo.json
│
├── docs/
│   ├── vision-spec.md             # the master vision doc
│   ├── prd.md                     # this document
│   └── adr/                       # architecture decision records, numbered
│       └── 0001-vcs-engine-jj-lib.md
│
├── crates/                        # Rust workspace members
│   ├── clotho-vcs/                # wraps jj-lib; gRPC service: init, commit, checkpoint, restore, op-log query
│   ├── clotho-storage/            # Arachne engine; wraps xet-core; chunk/xorb upload+download service
│   ├── clotho-merge-queue/        # multi-workspace reconciliation (the hardest, most novel piece)
│   ├── clotho-agent-gateway/      # MCP server; agent identity & permission enforcement
│   ├── clotho-diff/               # tree-sitter based structured diff engine
│   ├── clotho-api-gateway/        # edge REST/GraphQL aggregation service
│   └── clotho-common/             # shared protobuf-generated types, error types, tracing setup
│
├── apps/                          # pnpm workspace: deployable frontends
│   ├── web/                       # main product app shell (Next.js)
│   └── site/                      # marketing/teaser site (evolved from the launch page)
│
├── packages/                      # pnpm workspace: shared TS libraries
│   ├── ui/                        # design system — tokens, components, matches teaser page language
│   ├── sdk-js/                    # generated/typed client SDK for the API gateway
│   └── config/                    # shared eslint/tsconfig/tailwind config
│
├── collab/
│   └── forgejo/                   # git submodule pinned to a Forgejo release; patches in collab/patches/
│
├── infra/
│   ├── docker/                    # Dockerfiles per service
│   ├── k8s/                       # deferred until post-prototype
│   └── terraform/                 # deferred until post-prototype
│
├── scripts/                       # one-off setup/migration/seed scripts
└── proto/                         # shared .proto definitions consumed by crates/ and packages/sdk-js
```

---

## 4. Architecture map (prototype-scoped)

```
                 ┌────────────┐
   human / agent │  apps/web   │
                 └─────┬──────┘
                       │ REST/GraphQL
                 ┌─────▼──────────────┐
                 │ clotho-api-gateway  │──────► Forgejo API (issues/PRs/org)
                 └─────┬───────────────┘
          ┌────────────┼─────────────────┐
          │             │                  │
    ┌─────▼─────┐ ┌─────▼──────┐  ┌───────▼────────┐
    │ clotho-vcs │ │clotho-storage│ │clotho-agent-gw  │◄── MCP clients (agents)
    │ (jj-lib)   │ │ (xet-core)   │ │ + merge-queue   │
    └─────┬──────┘ └──────┬───────┘ └────────┬────────┘
          │                │                    │
     git objects      MinIO / S3         Postgres (identity, audit)
```

---

## 5. Development stages

Each stage lists a goal, key tasks, and an exit condition. Stages 1–2 can start immediately and in parallel; Stage 3 can start in parallel once Docker Compose skeleton exists.

### Stage 0 — Scaffolding (Day 0–1)
- Create the monorepo structure exactly as in §3.
- Root `Cargo.toml` workspace with empty crates (each with a trivial `lib.rs`/`main.rs` and a health-check).
- Root `pnpm-workspace.yaml`, empty `apps/web` (Next.js starter) and `packages/ui` (import the design tokens from the teaser HTML).
- `docker-compose.dev.yml`: Postgres, MinIO, and stub containers for each Rust service.
- ADR-0001: record the jj-lib decision and its rationale (from §2) formally.
- **Exit condition:** `docker compose up` brings up all containers cleanly; `just test` runs (even if trivially) across both workspaces in CI.

### Stage 1 — VCS engine core (Week 1–2)
- Embed `jj-lib` in `clotho-vcs`; expose gRPC: `init_repo`, `commit`, `checkpoint`, `restore_to`, `query_op_log`.
- Validate: programmatically create a repo, make commits from two separate simulated "agent workspaces" (using jj's workspace feature), confirm both land in one commit graph.
- Write integration tests against a real throwaway git remote.
- **Exit condition:** a test harness can create a repo, commit from two workspaces, and read a unified op log back — entirely through the gRPC API, no shelling out to the `jj` binary.
- *Implementation note (2026-07-07):* the engine landed **workspace-less** — commits are built server-side as trees directly via the store, so there is no working copy, no staging area, and no single-writer working-copy lock at all. Two simulated agents committing through the gRPC API into one graph is covered by `crates/clotho-vcs/tests/vcs.rs`; jj's on-disk workspace feature becomes relevant in Stage 5 if the merge-queue needs materialized working copies rather than pure tree ops.

### Stage 2 — Arachne storage engine (Week 2–4, parallel with Stage 1)
- Embed `xet-core` crates in `clotho-storage`; implement upload (chunk → xorb → S3 write) and download (reconstruct file from xorb + chunk ranges) against MinIO.
- **Exit condition:** upload a multi-GB synthetic file, then upload a near-duplicate (small modification) — confirm via storage metrics that only the changed chunks were newly written, and that download reconstructs both files byte-identical to source.
- *Implementation note (2026-07-07):* the "CAS shim" risk from §9 resolved cleanly — xet-core's published crates (`xet-data`, `xet-core-structures`, pinned `=1.5.2`) expose the dedup driver behind an explicit `DeduplicationDataInterface` trait, and our shim implements it over any S3-compatible store via the `object_store` crate (endpoint+credentials config only; see ADR-0002 amendment). The HF-service-specific crates (`xet-client`, `hf-xet`) are not used. Exit condition **measured** (`crates/clotho-storage/tests/storage.rs`, ground truth by listing the bucket): 2 GiB incompressible file, near-duplicate with a 64 KiB overwrite plus a 1 KiB insertion (shifting all subsequent bytes) grew the store by **262,734 bytes — 0.01% of the file** (33,219 of 33,222 chunks deduped); identical re-upload wrote 0 new chunk bytes; both files reconstructed byte-identical; dedup state survives engine restart from the bucket's `shards/` prefix alone. CI runs the same test against MinIO at 64 MiB.

### Stage 3 — Collaboration shell integration (Week 3–5, parallel)
- Stand up Forgejo via the `collab/forgejo` submodule + Docker Compose.
- Point Forgejo at git-compatible repos managed by `clotho-vcs` (Forgejo talks to them as ordinary git repos — no Forgejo code changes needed for this prototype stage).
- Wire repo creation in the API gateway to provision both a Forgejo repo entry and a `clotho-vcs` repo.
- **Exit condition:** creating a repo through Clotho's API produces a real Forgejo project with working issues/PRs, backed by a jj-managed git repo.
- *Implementation note (2026-07-07):* Forgejo is pinned to **v15.0.3** (current LTS; the v14 line hit EOL 2026-04-30) as both the `collab/forgejo` submodule and the unmodified official container image — zero Forgejo source changes. Wiring is **shared-git-root + adopt**, not push-mirror (gitoxide can't push, and pushing would need the git CLI, violating §6): `clotho-vcs` gained jj's *external* git backend (`CLOTHO_VCS_GIT_REPOS_DIR`) so backing bare git repos land on a volume Forgejo reads as its repository root, and the engine mirrors its `main` bookmark to `refs/heads/main` (+ HEAD) via gix after every commit/restore. `POST /api/v1/repos` on the gateway (Axum — the first real REST edge endpoint) does InitRepo → seed initial commit → Forgejo admin adopt; a one-shot provisioner creates the admin user and API token on first `docker compose up`. See ADR-0003 for the decision and its consequences (Forgejo-side writes bypass the jj op log until an import exists — deferred to Stage 5). Exit condition verified live and by `crates/clotho-api-gateway/tests/gateway.rs` (env-gated like the storage tests; `just test-collab` against the dev stack; CI runs it against a real Forgejo container): one API call → Forgejo project with a working issue and mergeable PR, and commits written through the vcs gRPC API render in Forgejo with no sync step.

### Stage 4 — Agent interface layer (Week 4–6)
- Build `clotho-agent-gateway` as an MCP server exposing `checkpoint`, `restore_to`, `diff_symbol`, `orient_repo`, backed by `clotho-vcs` and `clotho-diff`.
- Implement agent identity: scoped tokens, Postgres-backed, distinct from human OAuth, with per-call audit logging.
- **Exit condition:** a real MCP client (e.g., an agent session in a coding tool) can authenticate as a scoped agent identity, checkpoint work, intentionally break something, and restore — entirely through MCP tool calls.
- *Implementation note (2026-07-07):* the gateway is built on **rmcp** (the official MCP Rust SDK, pinned `=2.0.0`) over **streamable HTTP** on port 8090 — not stdio, since agents reach one shared, containerized service that enforces scopes centrally (ADR-0004). The third-party `agentic-jujutsu` crate was not used, per §9. `clotho-diff` graduated from stub to a real gRPC service (tree-sitter symbol diff, Rust + TypeScript/TSX for the prototype; other languages diff at file level), and `clotho-vcs` gained the read RPCs the tools needed: `GetHeads`, `ListFiles`, `DiffCommits` (changed files with full before/after contents — clotho-diff itself never touches storage, so the same structured-diff object can feed the Stage 6 PR view from any caller). Agent identity is three Postgres tables owned by the gateway (`agents`, `agent_tokens`, `agent_audit_log` — sqlx embedded migrations, ADR-0005): SHA-256-hashed bearer tokens scoped by `allowed_repos` + `allowed_tools`, checked on every call, with denied calls audited alongside successful ones; a token-guarded admin REST surface creates agents and mints tokens. Exit condition verified two ways: `crates/clotho-agent-gateway/tests/agent.rs` (env-gated; `just test-agent` against the dev stack; CI runs it with a real Postgres) drives a real MCP client (rmcp's own client over streamable HTTP — the same protocol path any coding tool uses) through mint → orient → checkpoint → break → diff\_symbol → restore plus scope-denial and bad-credential paths, verified both against the live dev stack and in CI.

### Stage 5 — Merge-queue prototype (Week 5–7)
- Naive-but-real version of multi-workspace reconciliation: two agent workspaces produce concurrent commits; the merge-queue serializes and rebases them, surfacing unresolvable conflicts as first-class jj conflict objects rather than blocking.
- **Exit condition:** two simulated agents committing concurrently to the same repo end up reconciled into one graph without a human intervening, for the non-conflicting case; conflicting case produces a clearly surfaced conflict commit.
- *Implementation note (2026-07-07):* write-time never blocks, land-time is serialized (ADR-0006). The engine's "main always advances to the newest commit" placeholder from Stage 3 got its real answer: `Commit` now only fast-forwards `main`, sibling commits coexist as jj's anonymous heads, and the only other way `main` moves is the new vcs `IntegrateCommit` RPC — fast-forward when possible, otherwise `jj_lib` rebase, where a conflicted rebase **lands anyway** as a first-class conflict commit (paths reported, main advanced, queue never blocked). `clotho-merge-queue` graduated from stub to a gRPC service: `SubmitChange` waits its turn on a per-repo async mutex and delegates to `IntegrateCommit` — the engine owns mutation, the queue owns ordering, and it is deliberately dumb (in-process lock, no persistence/batching/speculative CI, per §9). ADR-0003's other deferred gap also closed: the engine now runs `jj git import` semantics (`git::import_refs`) before every operation, so Forgejo-side writes (UI merges, pushes moving `refs/heads/main`) appear in the jj op log as `import git refs` operations instead of staying invisible. Exit condition verified by `crates/clotho-merge-queue/tests/queue.rs` (concurrent non-conflicting submissions reconcile into one graph unattended; same-file divergence lands as a surfaced conflict commit with `conflicted_paths`, and a follow-up commit integrates cleanly on top) and `crates/clotho-vcs/tests/vcs.rs` (external ref moves flow back into the op log); both run in plain `cargo test`, no stack required.

### Stage 6 — Frontend (Week 5–7, parallel)
- Evolve `apps/site` (teaser) design tokens into `packages/ui`.
- Build `apps/web`: repo browser, PR view (proxying Forgejo), basic agent-session presence panel (can be mocked/polling for the prototype, no need for real-time infra yet).
- **Exit condition:** a human can browse a repo, view a PR with a structured diff, and see which agent sessions have touched it recently.
- *Implementation note (2026-07-07):* the api-gateway grew first (ADR-0007) — the frontend talks **only** to it: repo list/detail, tree, file contents, commit log, and op log come from clotho-vcs (two new read RPCs, `GetFile` and `LogCommits`); PR list/detail proxy Forgejo; the structured PR diff is the *same* vcs `DiffCommits` → clotho-diff `DiffFiles` composition the MCP `diff_symbol` tool uses, plus edge-computed line hunks (`similar`) since humans read hunks while agents read symbols. Agent presence is proxied, not queried: the agent gateway (which owns the identity schema, ADR-0005) gained a per-repo sessions endpoint aggregating its audit log per (agent, token), and the web app polls it — no real-time infra, as this stage explicitly allows. ADR-0006's loose end got its scoped answer: vcs reads now **materialize** unresolved conflicts (jj marker text) and flag them end-to-end (tree → file → diff → UI badge + styled markers) instead of skipping them; the diff proto itself carries no conflict semantics. `apps/web` (Next.js on Kumo, Belweave black-and-white per `packages/ui` tokens) ships `/`, `/repos/[name]`, blob view, `/repos/[name]/pulls[/n]`; `@clotho/sdk-js` became a real hand-written typed client (OpenAPI generation deferred, ADR-0007) with vitest coverage. Exit condition verified live and by `crates/clotho-api-gateway/tests/stage6.rs` + a presence step in the Stage 4 agent test (`just test-collab` / `just test-agent`; CI runs both).

### Stage 7 — Pluggable compute + integration demo (Week 7–8)
- Integrate one CCI provider or compatible adapter for a real external sandbox
  provider.
- Wire a push webhook → CCI → sandbox run → status reported back to the PR.
- Run the full definition-of-done scenario from §1 end to end.
- **Exit condition:** the Stage 7 demo scenario runs live, recorded, and reproducible from a clean `docker compose up`.
- *Implementation note (2026-07-07):* the provider decision (§11 #3) was made by a human — **Daytona**, kept modular behind the CCI (docs/adr/0008). The CCI is a **Rust-native `ComputeProvider` trait** in a new `clotho-compute` crate (gRPC on :50057), not a TS worker wrapping ComputeSDK: no-lock-in means we own the swappable interface anyway, so a `DaytonaProvider` calling Daytona's REST API directly (control plane at `app.daytona.io/api` + toolbox proxy at `proxy.app.daytona.io`, both Bearer `DAYTONA_API_KEY`) keeps the backend all-Rust/gRPC — a second provider (E2B, …) is another impl of the same trait. With no key set the service runs a **disabled** provider (jobs fail `FAILED_PRECONDITION`) so plain `cargo test`/CI stay green; the round-trip test self-skips like the other env-gated ones (`just test-compute`). **Wiring:** Forgejo push webhook → api-gateway `/api/v1/webhooks/forgejo` (HMAC-verified, registered per repo at creation) → the CI job exports the repo's real git objects from clotho-vcs (new `ExportRepoArchive` RPC — a filesystem tar of the backing bare git repo, never a git shell-out) → clotho-compute ships them into a fresh sandbox, clones, checks out the pushed commit, and runs `.clotho/ci.sh` (else a default probe) → the exit code is reported back to the PR via Forgejo's commit-status API. Compute stays vendor- *and* collaboration-agnostic (it only runs commands); orchestration lives at the edge next to the Forgejo coupling. **Key deviation, ADR'd:** Daytona sandboxes run in Daytona's cloud with no route back to the local stack, so git objects are *shipped in* rather than fetched over git-http — the honest reproducible-from-a-clean-`up` choice (only a `.env` key needed), and it keeps the demo provider-agnostic. The definition-of-done demo is one command (`just demo`, driver `crates/clotho-demo`, `scripts/demo/run.sh`): two agents commit concurrently over vcs gRPC + merge-queue (reconciled unattended), a large binary uploads twice through the storage engine with measured chunk dedup, a PR opens for review at :3100, and the push-triggered CI job runs on the real Daytona sandbox and reports status. **Recorded follow-ups (not built — deliberately out of Stage 7 scope):** MCP `commit`/`submit_change` write tools routed through the merge-queue (the MCP surface is still read/checkpoint-only, so the demo's agents commit over raw vcs gRPC), and the post-prototype Rust `clotho` CLI (vision spec §5).

### Stage 8 — Post-prototype agent write path + `clotho` CLI
- Close Stage 7's ergonomics gap: agents can create commits and submit them to the merge queue entirely through MCP, under scoped identity and audit.
- Ship the first Rust `clotho` CLI promised by the vision spec §5.
- Keep the PRD honest with ADRs for the post-prototype decisions.
- **Implementation note (2026-07-08):** `clotho-agent-gateway` gained MCP `commit` and `submit_change` tools (docs/adr/0009). `commit` routes to `clotho-vcs.Commit` with explicit text file contents/deletions/parents/message and defaults author metadata from the authenticated agent; `submit_change` routes to `clotho-merge-queue.SubmitChange`, so landing still goes through the serialized queue from ADR-0006. The existing `allowed_tools` scope model now covers `commit` and `submit_change` by name; the existing audit log records writes, denials, and errors before responses return, so Stage 6 presence sees MCP writes instead of being blind to them. `open_pr` and `request_review` remain deferred until their api-gateway/Forgejo API shape is designed; Forgejo source remains untouched.
- **Implementation note (2026-07-08):** the new `crates/clotho-cli` binary (`clotho`) talks to the api-gateway REST edge, not internal gRPC or MCP (docs/adr/0010): `init`, `status`, `log`, `pr`, `commit`, and `submit` are implemented, with `commit --submit` as the simple author-and-land path. The edge grew `POST /api/v1/repos/{name}/commits` and `POST /api/v1/repos/{name}/submit` for this human-facing write path. The first CLI commit workflow is intentionally explicit/text-first (`--file <path>` repeated, optional `--delete`); recursive working-tree discovery, ignore semantics, binary/artifact commits, and CLI config files are later product work. No component shells out to `jj` or `git`.
- **Implementation note (2026-07-08):** PRD §11 decisions #1 (Clotho's license) and #2 (fork Forgejo vs stay API-level) remain open human decisions. Stage 8 does not change the repository's existing Apache-2.0 metadata and does not modify `collab/forgejo`.

### Stage 9 — Product shell + collaboration facade
- Make Clotho's web/API the primary collaboration surface while keeping Forgejo
  as the internal provider.
- Preserve the GPLv3/API boundary: no Forgejo source edits, no license
  decision, and no fork decision.
- **Implementation note (2026-07-08):** ADR-0011 records the Stage 9 facade
  decision. `clotho-api-gateway` now exposes Clotho-owned issue, issue
  comment, pull comment/review/merge, branch, and commit-status routes backed
  by Forgejo. `@clotho/sdk-js` mirrors those routes with typed methods and
  tests. `apps/web` now has a denser repo shell with Code, Pull Requests,
  Issues, Checks, Agents, Storage, Insights, and Settings sections; native
  issue list/create/detail/comment pages; PR checks, changed-file navigation,
  review/comment forms, merge action, and agent provenance surfaces. Normal web
  flows no longer link users to Forgejo.
- **Implementation note (2026-07-08):** the normal dev Compose port for Forgejo
  moved from host `3000` to `13000`; internal container traffic remains
  `http://forgejo:3000`. A currently running dev stack must be recreated
  non-destructively (for example `docker compose -f docker-compose.dev.yml up
  -d forgejo`) before local tests use the new host port. Do not run
  `just dev-down` just to apply this change.

### Stage 10 — Actions + Compute Control Plane
- Make Actions a first-class Clotho product surface backed by the existing CCI
  compute integration.
- Keep Daytona as the first real provider behind `clotho-compute`; do not
  hardcode the UI or API to Daytona beyond provider metadata/config defaults.
- Treat commit statuses as compatibility output for PRs, not the primary
  Actions record.
- **Implementation note (2026-07-08):** ADR-0012 records the Actions control
  plane decision. `clotho-api-gateway` now owns `/actions` run, log, config,
  and compute-provider JSON routes. Push-triggered CI creates a Clotho action
  run, marks it running, records logs/provider/sandbox/result metadata, and
  still syncs Forgejo commit statuses for PR compatibility. Manual runs can be
  started from the Clotho API/web app. The first run store is intentionally
  gateway-local so the API/UI contract can settle before adding Postgres
  persistence or a separate `clotho-actions` service.
- **Implementation note (2026-07-08):** `apps/web` renamed the repo `Checks`
  section to `Actions`, added run list/detail/log pages, and surfaces
  runner/sandbox configuration in settings without returning secret values.
  `@clotho/sdk-js` gained typed Actions/config/provider methods and tests.
  Forgejo remains unmodified and internal.

### Stage 11 — Core Control Plane Maturity
- Add Clotho-owned users, orgs, teams, memberships, repo ownership, and
  permissions in Postgres. Clotho identity becomes the v2 source of truth;
  Forgejo identity is an internal/provider mapping only.
- Add web repo creation, org/repo dashboards, mature repo settings, provider
  status, clone URLs, default branch controls, and visibility metadata.
- Keep secrets environment-backed for now. Settings UI may show masked
  configured state and non-secret config only; encrypted secret storage is a
  later design, not an implicit Stage 11 requirement.
- Add a Clotho activity/audit model that can power dashboards, leaderboards,
  notifications, agent provenance, and security views.
- **Acceptance:** a user can create/manage repos from the web app without
  `curl`, see org/repo settings, and understand ownership/permissions without
  opening Forgejo.

### Stage 12 — Compute Platform v2
- Extend CCI from one-shot `run_job` into a capability-aware provider registry:
  provider id, configured state, supported features, regions,
  snapshots/templates, persistence, SSH, desktop, public URL, file APIs,
  terminal streaming, and cost hints.
- Keep the current Daytona provider as the stable direct Rust provider.
- Add a ComputeSDK bridge service as an optional provider implementation behind
  CCI for broad provider support and routing/failover. Current ComputeSDK docs
  describe a TypeScript sandbox API with provider packages for services such as
  Cloudflare, Daytona, E2B, Modal, Namespace, Vercel, and others, plus
  multi-provider strategies including priority, round-robin, and fallback on
  error: https://docs.computesdk.com/getting-started/introduction
- Add a Box provider adapter after the provider registry exists. Model Box as a
  persistent agent workspace / long-running sandbox: its public docs describe
  persistent Ubuntu VMs with SSH/SCP, snapshots/forks, command execution,
  prompt runs, events, desktop streaming, public hosting, and file/artifact API
  surfaces: https://box.ascii.dev/ and https://docs.ascii.dev/box/api/v1
- Add web settings pages for compute providers and Actions defaults, without
  exposing raw secrets.
- **Acceptance:** Clotho can list multiple configured providers, route
  Actions/sandbox requests by capability, and expose provider state
  consistently through API/web/SDK.
- *Implementation note (2026-07-08):* ADR-0013 records the Stage 12 shape.
  **`clotho-compute`** owns a multi-provider `ProviderRegistry` with structured
  capabilities; gRPC adds `ListProviders` / `GetProvider`, and `RunJob` accepts
  optional `provider_id` (empty → default + first configured one-shot).
  Providers: **Daytona** (direct Rust, unchanged path), **computesdk** (optional
  TypeScript HTTP sidecar at `services/compute-sdk-bridge`, disabled without
  `CLOTHO_COMPUTE_SDK_BRIDGE_URL`; uses ComputeSDK
  [docs](https://docs.computesdk.com/llms.txt) multi-provider
  priority/round-robin when packages + keys are present), **box** (stub with
  honest caps from Box API v1 at `https://ascii.dev/api/box/v1`,
  [docs](https://docs.ascii.dev/llms.txt); full client deferred).
  **api-gateway** exposes `/api/v1/providers` (+ Stage 10
  `/api/v1/compute/providers` alias), proxies registry metadata (env fallback),
  and routes Actions CI with `provider_id` from repo Actions config — no
  Daytona hard-coding. **Web:** `/settings/compute` + repo settings provider
  list (masked/configured only). **SDK:** `computeProviders` /
  `computeProviderList` / richer `ComputeProvider`. Stage 12 secrets were
  env-backed; Stage 13 moved primary credentials to Clotho secrets (ADR-0014).
  PRD §11 #4 resolved (TS sidecar).

### Stage 13 — Web Product Expansion
- Build first-class pages for: new repo, org dashboard, repo settings sections,
  provider settings, agent management, activity feed, notifications, branches,
  commits, releases/artifacts, richer storage, richer insights, and account/org
  settings.
- Upgrade Issues and PRs: labels, assignees, milestones, filtering, saved
  views, review threads, checks/actions panels, branch/merge policy, and agent
  provenance.
- Add command palette, keyboard navigation, dense tables, empty/error/loading
  states, and consistent Kumo-based page patterns.
- **Acceptance:** normal GitHub/GitLab repo workflows are possible from
  Clotho's web UI without Forgejo.

#### Stage 13 implementation notes (2026-07-08)

Shipped a console-quality web redesign plus first-class secrets:

- **IA / shell:** `AppShell` global nav (dashboard, repos, agents, activity,
  settings), mobile drawer, ⌘K command palette, settings hub.
- **Craft:** larger body type (~15px), `PageFrame` / `PageTitle` / `EmptyState` /
  `SettingsSection`, redesigned dashboard and repo overview; product copy only
  (no Forgejo/internal host advertising); clone URLs sanitized to public hosts.
- **Routes:** `/`, `/repos`, `/repos/new`, `/settings`, `/settings/compute`,
  `/settings/secrets`, `/agents`, `/activity`, `/orgs`, `/orgs/[org]`; modular
  repo settings (general, collaborators, secrets, actions, compute, danger).
- **Secrets (docs/adr/0014):** Postgres table + AES-256-GCM seal
  (`CLOTHO_SECRETS_MASTER_KEY`); REST at `/api/v1/orgs/{org}/secrets`,
  `/api/v1/repos/{repo}/secrets`, `POST /api/v1/providers/{id}/connect`;
  metadata-only responses; activity audit events; SDK parity.
- **Compute wiring:** gateway resolves provider keys from secrets into CCI
  `RunJob.provider_credentials`; Daytona accepts per-job keys when env empty;
  provider list overlays Clotho-secret configured state. `.env.example`
  documents bootstrap secrets; provider keys are secondary escape hatches.
- **Deferred within Stage 13:** full issue/PR upgrade (labels/milestones/…),
  notifications, branches/commits/releases pages, OpenAPI generation.
  **Stage 14 completed** Box HTTP client + ComputeSDK secret/bridge maturity
  (see Stage 14 implementation notes).

### Stage 14 — Platform Hardening & Honest Compute *(inject — recommended next for ops honesty; can run in parallel with Stage 15 start)*
Short housekeeping stage so the multi-provider story is truthful and secrets
are first-class for every advertised provider — not only Daytona.

- **Box (Ascii) completion:** replace the Stage 12 stub with a real CCI
  provider against `https://ascii.dev/api/box/v1` (create → files/commands →
  tear down for one-shot; design hooks for persistent workspace lifecycle).
  Accept per-job `provider_credentials.api_key` from the gateway secrets path
  (same pattern as Daytona, docs/adr/0014). Do not claim “configured” unless
  jobs can actually run.
- **ComputeSDK bridge maturity:**
  - Document and ship a compose profile / `just` target that runs
    `services/compute-sdk-bridge`.
  - Allow Clotho-stored secrets for bridge config and common upstream keys
    (e.g. `E2B_API_KEY`, `MODAL_TOKEN_*`) with gateway → sidecar injection or
    documented sync — never return raw values to the browser.
  - Settings UI: connect/configure bridge (URL optional if in-cluster default)
    without requiring host `.env` as the only path.
- **Honest provider state:** unify env, Clotho secrets, and stub/disabled
  reasons across compute gRPC, REST, web, and SDK. “Configured” always means
  “can accept a job with current credentials.”
- **Product leaks sweep:** audit remaining operator-facing copy for Forgejo,
  docker hostnames, raw env-only instructions; keep advanced env notes in
  docs only.
- **Bootstrap ops:** document/generate `CLOTHO_SECRETS_MASTER_KEY` safely
  (never paste shell command literals into `.env`).
- **Optional light REST gaps:** if needed for Stage 15, add thin routes for
  provider disconnect and secret rotation metadata only — no discovery work.
- **Acceptance:** Daytona, Box, and ComputeSDK each have a clear path from
  “not connected” → “connected via Clotho secret or documented bootstrap” →
  “job fails only for real provider errors.” No provider card lies.

#### Stage 14 implementation notes (2026-07-08)

- **Box (Ascii):** real CCI provider in `crates/clotho-compute` against
  `https://ascii.dev/api/box/v1` — create → poll ready/idle → write files
  (base64) → `POST …/commands` → delete. Per-job `provider_credentials.api_key`
  (gateway secrets path). Persistent hooks (`create_persistent`, `stop_box`,
  `resume_box`) for a later session API. `configured` only when env key present
  or Clotho secret overlay; never while stub-only. Note: Box API caps command
  `timeoutSeconds` at 60.
- **ComputeSDK bridge:** compose profile `compute-bridge` + `just dev-compute-bridge`
  (pnpm-only image/workspace). Catalog of **all** ComputeSDK upstreams
  (AgentCore, Agentuity, Archil, Beam, Blaxel, Cloudflare, CodeSandbox, Daytona,
  Declaw, E2B, Freestyle, HopX, k8s, Leap0, Modal, Namespace, Runloop,
  Tensorlake, Upstash, Vercel) in bridge `providers.mjs` + gateway
  `computesdk_catalog`. `GET /api/v1/providers/computesdk/upstreams` for UI.
  Connect any upstream via `credentials` map; secrets inject as UPPER_SNAKE
  env names on jobs. Live `/health` for honest `configured`.
- **Honest state:** “configured” = can accept a job with current credentials
  (env, Clotho secret inject, or bridge upstream). Bridge URL alone is not
  configured. REST overlay unifies gRPC + secrets for web/SDK.
- **REST:** `DELETE /api/v1/providers/{id}/connect` disconnects (deletes
  well-known secrets); connect extended for computesdk. SDK:
  `disconnectProvider`. OpenAPI updated.
- **Bootstrap:** `.env.example` documents safe `CLOTHO_SECRETS_MASTER_KEY`
  generation (paste 64 hex chars; never shell command literals).
- **Not in this slice:** Stage 16 discovery/social; sandbox session REST;
  full Modal connect form in web (secrets page + connect API fields work).

### Stage 15 — World-Class API / CLI / SDK / MCP Parity *(was Stage 14; elevated priority for agent-native thesis)*
Make REST the single product contract; every stable web capability must be
reachable by humans (CLI) and agents (MCP) with the same semantics as the SDK.

#### Current surface audit (2026-07-09, Slice F) — start here

| Surface | What exists today | Gaps |
|---|---|---|
| **REST** (`clotho-api-gateway`) | health; auth (`/me`, `/tokens`); users/orgs/activity; repos CRUD + PATCH/DELETE + merge-policy; tree/file/commits/oplog/submit; issues/PRs/comments/reviews/merge/diff; labels/milestones/assignees; notifications; branches; statuses; Actions runs/logs/config; providers + connect; agent-sessions; secrets (org/repo); **agent admin via edge** (`/agents`, tokens, audit); **OpenAPI at `/openapi.yaml` + `docs/openapi.yaml`** | No public `/sandboxes` session API; no signals; clone URL may still need a Clotho-public git endpoint; internal collab webhook only |
| **SDK** (`@clotho/sdk-js`) | Hand-written client covering REST above incl. auth, agents, labels, notifications, merge-policy, secrets, connect | No OpenAPI→TS codegen yet (path drift CI exists); no sandbox session types |
| **CLI** (`clotho`) | Grouped `auth|repo|issue|label|milestone|notification|pr|actions|provider|secret|org|activity|agent` + `--json`; Stage 8 aliases; demo loop documented | Recursive working-tree commit still later product work |
| **MCP** (`clotho-agent-gateway`) | VCS tools (gRPC) + collab/Actions/platform/read helpers **via REST edge**; `create_issue` supports labels/assignees/milestone | No sandbox sessions; secrets are list/metadata only (no write tools by design); **no agent-admin mint tools** (by design) |

#### Stage 15 workstreams

1. **Contract-first REST**
   - Publish OpenAPI 3 for `/api/v1/*` (generated from Axum routes or a single
     hand-maintained `openapi.yaml` that CI checks against).
   - Generate or validate `@clotho/sdk-js` against that contract; fail CI on drift.
   - Document auth model and the versioned stable error envelope with request
     correlation (Stage 22 supersedes the original `{ "error": "..." }`).
   - Fill REST gaps needed for CLI/MCP parity before inventing CLI-only behavior:
     at minimum stable list/get/create for issues, PRs, Actions, providers,
     secrets metadata, activity; optional `/api/v1/sandboxes` only if CCI
     session APIs exist (else defer sandbox sessions to post-Box Stage 14).

2. **CLI maturity (`clotho`)**
   - Group commands: `clotho repo|issue|pr|actions|provider|secret|org|agent|activity`.
   - Every command is a thin REST client (same as today: no git/jj shell-out).
   - `--json` machine output; `--api` / `CLOTHO_API_URL`; exit codes for scripts.
   - Cover the human “daily path”: create repo, open issue, open/review/merge PR,
     start Action + tail logs, list providers, set/list secrets (write-only value),
     show activity.
   - Ship `clotho help` / man-page quality usage; add `crates/clotho-cli` tests
     against a mock or gateway fixture.

3. **MCP maturity (agent-native)**
   - Expand tools so an agent can operate a repo end-to-end without raw REST:
     - Collab: `list_issues`, `create_issue`, `comment_issue`, `list_pulls`,
       `create_pull`, `comment_pull`, `review_pull`, `merge_pull`
     - Actions: `list_action_runs`, `start_action_run`, `get_action_logs`
     - Platform: `list_providers`, `list_repos`, `get_activity` (and optionally
       `list_secrets` metadata-only — never secret values)
     - Read helpers: `get_tree`, `get_file` if not already covered by orient
   - Prefer implementing new tools **through the REST edge** (or a shared Rust
     client crate) so MCP cannot drift from the public API.
   - Keep token scopes (`allowed_tools`, `allowed_repos`) and audit log for every
     new tool; document the tool list in MCP server instructions.
   - Tests: permission denied paths + happy path for each new tool family.

4. **SDK parity & docs**
   - SDK method for every stable REST route used by web or CLI.
   - Short `docs/api.md` + `docs/cli.md` + `docs/mcp.md` (or one “Developer
     surfaces” doc) with copy-paste examples for human, script, and agent.
   - Versioning policy: additive REST for minor; breaking changes only with
     explicit major bump once out of prototype.

5. **Acceptance (strict)**
   - A human can perform the “demo loop” (create repo → issue → PR → Action →
     inspect logs → list providers) via **CLI only**.
   - An AI agent with a scoped token can perform the same loop via **MCP only**.
   - SDK tests cover every public method; OpenAPI (or equivalent) exists and is
     CI-checked; no feature is marked mature if it is web-only.

#### Stage 15 implementation notes (2026-07-09, Slices A–F)

- **OpenAPI:** hand-maintained [`docs/openapi.yaml`](openapi.yaml) covers stable
  `/api/v1/*` routes + error envelope; served at `GET /openapi.yaml`; path drift
  checked by `crates/clotho-api-gateway/tests/openapi_drift.rs`. Auth blurb
  documents `CLOTHO_AUTH_REQUIRED` + Bearer tokens + agent admin on edge.
- **Auth (A):** human API tokens, `/me`, `/tokens`, repo PATCH/DELETE,
  Clotho-only `info` field on repo detail.
- **CLI:** regrouped into `auth|repo|issue|label|milestone|notification|pr|
  actions|provider|secret|org|activity|agent` with `--json` and Stage 8 aliases.
  Demo loop in [`docs/cli.md`](cli.md). Still REST-only (ADR-0010).
- **Agent admin (C):** edge-proxied `/api/v1/agents/*`; CLI + web; MCP has no
  mint tools (ADR-0016).
- **Collab depth (D):** labels, milestones, assignees on issues; notifications.
- **Merge policy (E):** `GET/PUT …/merge-policy`; honest review threads;
  merge 409 envelope.
- **MCP:** collab, Actions, platform, and read helpers call the **REST edge**
  via `CLOTHO_API_URL`. VCS tools remain gRPC. Docs: [`docs/mcp.md`](mcp.md).
- **SDK:** covers the REST surface including agents, notifications, merge-policy;
  22 tests in `packages/sdk-js`. Docs: [`docs/api.md`](api.md).
- **Not in Stage 15:** sandbox session API, OpenAPI→SDK codegen, signals/discovery
  (Stage 16), full Box/ComputeSDK honesty (Stage 14).

### Stage 16 — Discovery, Social, and Competitive UX *(was Stage 15)*
- Add Clotho's GitHub Stars competitor as `Signals`: users/orgs can signal
  repos, optionally categorize them, and use signals for discovery/ranking.
- Add repo/user/org profiles, public/private visibility, trending repos,
  activity timelines, contribution/commit leaderboards, agent leaderboards, and
  "most active / most reliable / most reviewed" views.
- Add commit-history leaderboards built from Clotho's own commit/provenance
  data, not scraped UI state.
- **Acceptance:** Clotho has a credible discovery/community layer while
  preserving enterprise/self-host privacy controls.
- **Prerequisite:** Stage 15 developer surfaces stable enough that discovery
  APIs (`/signals`, profiles) land with SDK/CLI/MCP stubs from day one.
  **PRD v3:** also prefer Stages 17–20 (trust, storage, network, agent runtime)
  far enough along that public discovery does not advertise empty model hosting
  or bolted-on compute.

### Stage 17 — Trust foundation (AuthProvider + Provider Fabric skeleton) *(PRD v3 Phase A)*
- Introduce `AuthProvider` (docs/adr/0018): `bootstrap` for local/dev;
  **Clerk** for managed human SSO, orgs, and human API keys.
- Keep agent identity Clotho-owned (ADR-0005); map `clerk_org_id` /
  `clerk_user_id` into Clotho orgs/users; never model agents as Clerk users.
- Production/managed profiles: auth required by default
  (`CLOTHO_AUTH_REQUIRED=true`, `CLOTHO_AUTH_PROVIDER=clerk`).
- Introduce **Provider Fabric** skeleton (docs/adr/0019): shared connect /
  disconnect / configured / capabilities pattern across compute (exists),
  storage, and network layers — stubs acceptable for storage/network until
  Stages 18–19.
- **Acceptance:** a human can sign in via Clerk on web, act with org context,
  and call REST with a human credential; agents still authenticate only with
  `clotho_agt_…` tokens; OpenAPI/SDK/CLI document the auth model.
- *Implementation note (2026-07-09):* `AuthProvider` lands in
  `clotho-api-gateway` (`auth_provider::{bootstrap,clerk}`) with
  `CLOTHO_AUTH_PROVIDER` + link tables (`clerk_user_links`, `clerk_org_links`).
  §11 #7 default: keep minting Clotho `clotho_tok_…` under both providers;
  Clerk sessions/keys also resolve. Fabric: `GET /api/v1/providers?layer=` +
  `?all=true`; storage/network honest stubs. Web: optional `@clerk/nextjs`
  when publishable key set. Tests: `auth_slice_a` + `auth_clerk` mocks.

### Stage 18 — Arachne on the VCS path + BYO object store *(PRD v3 Phase B)*

*Implementation note (2026-07-11):* the first production path is live. REST,
SDK, and CLI commits accept UTF-8 or base64 payloads; payloads at the default
10 MiB threshold are uploaded to Arachne and committed to jj/git as standard
git-LFS pointers with a Clotho Arachne hash extension. File reads reconstruct
the payload and verify its size and SHA-256 before returning it. The storage
fabric probes live Arachne state. An optional open StorageSDK bridge supplies
S3/MinIO/R2/filesystem adapters and snapshot/fork primitives for agent artifact
namespaces. Remaining Stage 18 work: org/repo BYO credential persistence,
repo kinds/policies, materialized CI export, and streaming download/LFS Batch.
- Ship `ObjectStoreProvider`: org (optional repo) BYO S3/R2/GCS-compatible
  bucket via secrets (ADR-0014/0019); MinIO remains the managed default.
- Wire Arachne into commit/fetch for large files; git-LFS pointer bridge at
  the edges (docs/adr/0020).
- Add repo `kind`: `code` | `model` | `dataset` with kind-tuned attributes
  and storage UI (dedup metrics, not only VCS tree).
- **Acceptance:** multi-GB model committed twice (near-duplicate) through
  normal API shows Stage-2-class chunk dedup; clone/fetch reconstructs
  byte-identical files; storage settings show honest configured state for
  BYO buckets.

### Stage 19 — Tailscale NetworkProvider + BYOC runner *(PRD v3 Phase C)*
- Org **Connect Tailscale**: OAuth client in secrets; suggested tags + ACL
  snippet; repo network policy (docs/adr/0021).
- **Private reach:** Actions/sandbox jobs join the customer tailnet as
  ephemeral tagged nodes when `private-net` is required.
- Ship `clotho-runner` binary: customer devices register as CCI providers
  (`byoc:…`) with capability ads; jobs route through CCI only.
- Private-cloud mode remains documented intent (control plane orchestrates;
  data/compute in-tailnet) — full packaging may follow.
- **Acceptance:** with Tailscale connected, a CI job reaches a private
  service only via the tailnet; a BYOC runner appears in provider list as
  configured and can run a job; disconnect clears credentials; demo path
  still works without Tailscale.

### Stage 20 — Agent runtime v2 *(PRD v3 Phase D)*
- Durable merge-queue (Postgres): survive restarts; queue visibility via
  REST; speculative CI before advancing `main` (docs/adr/0022).
- Public `/api/v1/sandboxes` session API backed by CCI persistent providers
  (Box first); MCP tools via REST edge; checkpoint/restore linked to sessions.
- Provenance trailers on agent commits (`Clotho-Agent`, run/session ids,
  optional prompt hash); merge policy may require human review for
  machine-authored commits.
- Symbol-aware merge explicitly deferred after this stage.
- **Acceptance:** two agents submit concurrently across a gateway restart
  and reconcile; failed speculative CI blocks land; sandbox checkpoint →
  restore round-trip works; MCP `commit` writes provenance trailers
  visible in web/CLI.

### Stage 21 — Discovery after foundations *(PRD v3 Phase E)*
- Execute Stage 16 Signals/profiles/leaderboards only when Stages 17–20
  acceptance is largely met (or explicitly waived per surface).
- Publish open performance benchmarks (clone/push vs git+LFS and HF Hub)
  once Arachne-on-VCS is real — performance-as-marketing.
- **Acceptance:** same as Stage 16, plus model/code public browsing does
  not lie about storage or compute capabilities.

### Stage 22 — Public alpha and contract hardening *(PRD v4 — immediate next gate)*

- Execute the P0 checklist in
  [`release-readiness.md`](release-readiness.md): complete OpenAPI schemas,
  stable error codes, pagination, idempotency, request correlation, generated or
  structurally verified SDK types, CLI automation semantics, versioned MCP tool
  contracts, authorization matrix, backup/restore drills, migrations, resource
  bounds, accessibility, packaging, and public project hygiene.
- Add the agent-ready repository contract: root `AGENTS.md`, current handoff,
  capability discovery, deterministic fixtures, bootstrap diagnostics, and an
  acceptance checklist that an unfamiliar agent can execute without internal
  services or undocumented environment variables.
- Treat the dashboard contrast issue as a design-system release blocker. Meet
  WCAG 2.2 AA in light and dark themes and capture release evidence across core
  journeys (see `docs/design/stage13-web-console.md`).
- Publish an honest alpha support/compatibility policy and known limitations;
  do not claim HA or production readiness without the later stable gate.
- **Acceptance:** clean clone succeeds on a new machine; all default tests and
  release builds pass; restore succeeds from a complete backup; API diff is
  reviewed; and a fresh agent completes create → orient → change → test →
  submit using only public docs and scoped surfaces.

### Stage 23 — Production identity, authorization, and tenant isolation

- Make an organization the explicit security, policy, storage, quota, billing,
  audit, and workload boundary. Every durable resource and indirect lookup must
  carry an immutable Clotho tenant/org identity.
- Complete multi-user organization membership: invitations, lifecycle, org
  switching, owner/admin/member/guest/service roles, repository grants, and
  human-only delegation/impersonation controls.
- Keep authentication pluggable: generated bootstrap identity for local/CI,
  Clerk for hosted Clotho, and generic OIDC for production self-hosting. After
  authentication, Clotho remains the authorization source. Agents remain
  Clotho-native identities and never become human IdP users.
- Require a typed tenant context in repositories, artifacts, releases, imports,
  Actions, tasks, logs, secrets, audits, providers, queues, caches, storage keys,
  webhooks, background jobs, and timing-safe indirect-ID resolution. Add
  PostgreSQL row-level security as defense in depth, not as a substitute for
  application checks.
- Publish and test the route/tool permission matrix. Default deny, tenant-safe
  not-found behavior, token expiry/rotation, session revocation, audit export,
  and cross-tenant cache/queue/storage isolation are release requirements.
- **Acceptance:** two adversarial organizations exercise every public resource
  family and cannot infer, read, mutate, schedule, or exhaust one another's
  state through direct IDs, pagination, timing, logs, storage, agents, provider
  credentials, webhooks, caches, or background work.

### Stage 24 — Hosted control plane, self-host profiles, and elastic workloads

- Separate the durable tenant-aware control plane from elastic workload cells.
  API, MCP, and web replicas are stateless; Postgres, queues, Git/VCS state,
  Arachne/object storage, and reconciliation have explicit ownership and
  recovery contracts.
- Add durable idempotent scheduling and leases for Actions, imports, merge work,
  agent tasks, and provider operations. Autoscale disposable workload workers by
  queue depth, latency, capability, region, and provider capacity—never pretend
  a stateful service becomes safe merely by adding replicas.
- Enforce per-tenant concurrency, API, storage, network, GPU, cost, and retry
  quotas with fair scheduling, admission control, backpressure, metering, and
  noisy-neighbor isolation. Use cells to bound tenant and regional blast radius.
- Deliver supported deployment profiles from the same source and contracts:
  Compose for evaluation/small installations; documented Helm/Kubernetes for
  production; external Postgres and object storage without product forks.
- Add HA and operational evidence: online migrations, connection pooling,
  rolling/mixed-version upgrades, point-in-time recovery, destructive restore
  drills, key rotation, reconciliation after dependency loss, SLOs, traces,
  metrics, alerts, capacity guidance, incident runbooks, SBOMs, provenance, and
  signed artifacts.
- **Acceptance:** a multi-replica hosted installation survives API and worker
  termination plus a rolling upgrade without losing accepted work; load scales
  workload cells up and down; tenant fairness and quotas hold under contention;
  and a second operator installs, upgrades, backs up, and restores both the
  production self-host profile and a representative hosted cell.

### Stage 25 — Handoff Capsules and repository task plane

- Make a handoff an immutable repository object: goal, authority, acceptance,
  checkpoint, operation/diff state, context manifest, evidence, budget, leases,
  related resources, and explicit assumptions/blockers.
- Add a task plane above issues for executable work with dependency graphs,
  concurrency-safe workspace leases, authority levels, bounded retries,
  speculative attempts, review routing, and terminal evidence.
- REST owns capsule/task semantics; CLI and MCP expose resume/fork/inspect;
  web renders the same object for humans.
- **Acceptance:** an agent without transcript access resumes another agent's
  partial task from a capsule and reaches the same deterministic test result;
  stale-base, cross-tenant, and insufficient-scope resumes fail explainably.

### Stage 26 — Lachesis evidence graph, release contracts, and Signals

- Compose source, artifacts, datasets, model ancestry, evaluations, Actions,
  compute/runtime facts, approvals, security/license results, SBOMs, signatures,
  deployments, and agent provenance into a content-addressed release graph.
- Add impact queries by digest and explainable release policies for required
  artifacts, evaluation thresholds, regression budgets, review, licensing,
  scanner state, reproducibility, and approved trust/network boundaries.
- Add **Signals**, Clotho's lightweight public repository-interest primitive.
  One authenticated human or organization may Signal a visible repository with
  optional intent `interested`, `using`, or `building_on`. Following for private
  notifications is separate. Signals grant no authority, never satisfy trust or
  release policy, respect visibility/deletion/moderation boundaries, and remain
  distinct from evidence-derived dependents, builds, and deployments.
- Generate common policies from web defaults; raw configuration remains an
  advanced escape hatch, not the onboarding path. Do not add global ranking or
  discovery until abuse resistance and permission-safe aggregation are proven.
- **Acceptance:** a user or agent can prove what produced a release, compare
  evaluation evidence fairly, and enumerate releases affected by a revoked
  dataset, vulnerable component, or failed policy edge. Public Signal totals
  reveal no private repository or tenant metadata, and evidence-derived adoption
  cannot be forged by clicking Signal.

### Stage 27 — Compute Bindings and GPU/data locality

- Add a repository/branch/release compute binding expressed as capabilities
  (accelerator, persistence, region, isolation, private reach, residency,
  budget), resolved through CCI rather than provider syntax.
- Materialize verified releases into persistent provider snapshots; fork warm
  evaluation/inference workspaces; cache by release/runtime/driver/GPU contract.
- Schedule near Arachne/BYO storage or inside the tailnet and show predicted
  transfer, warmup, runtime cost, and trust boundary before execution.
- Return outputs to tenant-isolated Arachne namespaces; repository mutation
  still passes policy and merge queue.
- **Acceptance:** a release-bound H100 workspace starts from a verified warm
  snapshot, performs no unnecessary full artifact transfer, and returns an
  attested result linked into Lachesis.

### Stage 28 — Lazy repositories and protocol mesh

- Materialize repository metadata immediately and fetch Arachne ranges on
  demand in managed sandboxes; later expose a cross-platform virtual mount.
- Add sparse path/symbol orientation for agents and signed offline travel packs.
- Generalize the existing Hugging Face projection so one immutable Clotho
  release can be consumed through Git/LFS, Hub, HTTPS ranges, OCI
  artifacts/referrers, and selected package/storage read protocols.
- **Acceptance:** a hundreds-of-gigabytes repository becomes usable without a
  full checkout, and every protocol resolves to the same Clotho release digest
  and evidence graph.

### Stage 29 — Connector/provider kit, Atropos lifecycle, and federation

- Publish out-of-process provider and policy SDKs with signed manifests,
  capability/egress declarations, UI schemas, and automated conformance suites.
- Add repository-bound database/warehouse/vector/object connectors: schema and
  bounded context, read-only by default, audited and network-policy constrained.
- Introduce Atropos retention, legal hold, garbage collection, cache eviction,
  revocation, and verifiable deletion across Git, Arachne, backups, and external
  providers.
- Add federation/discovery only after permission, evidence, moderation, Signals,
  and lifecycle semantics remain correct across instances.
- **Acceptance:** a third party ships a conformant integration without core
  patches; lifecycle policy traces or deletes every relevant copy; federation
  never leaks private metadata, inflates Signals, or bypasses release
  verification.

---

## 6. Success criteria

### Prototype

- All Stage 0–7 exit conditions met.
- No component depends on shelling out to the `jj` or `git` CLI binaries at runtime — everything goes through `jj-lib`/`gitoxide` embedded APIs. (The external CI *sandbox* runs `git clone` on the objects Clotho ships it — that's the CI job, not a Clotho service.)
- Storage dedup is *measured*, not assumed — the demo shows a real before/after byte count (`just demo`, Stage 2/7 notes).
- At least one agent identity is fully distinct from a human identity in the data model, not a flag on a user row.
- Compute is provider-agnostic: the one integrated provider (Daytona) sits behind the CCI trait, with no vendor baked into any caller (docs/adr/0008).
- The whole stack runs from a single `docker compose up` on a laptop; the end-to-end demo needs only a `DAYTONA_API_KEY` in `.env` for the CI leg.

### PRD v2

- Forgejo remains an internal provider, unmodified, and hidden behind
  Clotho-owned APIs for normal user and agent workflows.
- Clotho-owned identity, repo ownership, permissions, activity, and provider
  state live in the Clotho control plane, not in scraped Forgejo UI state.
- CCI remains the compute boundary. Direct providers, ComputeSDK bridge
  providers, Box, and future providers are all implementations behind CCI.
- REST is the canonical public API. Web, SDK, CLI, and MCP surfaces wrap it or
  share its typed contract instead of drifting into separate behavior.
- No Clotho service shells out to `git` or `jj` for product behavior.

### PRD v3 (Dream Roadmap)

- **Provider Fabric:** auth, compute, storage, and network follow one
  connect/configured/capabilities pattern (ADR-0019). Humans never need
  vendor names for the common path — they declare capabilities.
- **Auth:** Clerk (or later OIDC) for humans via AuthProvider; agents remain
  Clotho-native (ADR-0018). Managed deployments require auth by default.
- **Storage:** Arachne is on the commit/fetch path; BYO object store works;
  `code` | `model` | `dataset` repo kinds are first-class (ADR-0020).
- **Network:** Tailscale org connect enables private-reach CI and BYOC
  runners behind CCI (ADR-0021) without making Tailscale mandatory for demos.
- **Agent runtime:** durable merge-queue, sandbox sessions, and commit
  provenance exist as product surfaces — not demo-only gRPC scripts
  (ADR-0022). Failure mode B (agents fight the queue; compute feels bolted
  on) is treated as a release blocker for "platform" claims.
- Stage 16 discovery does not ship ahead of Stages 17–20 without an
  explicit waiver.

### PRD v4 (Public trust + frontier)

- Stage 22 public-alpha gates are evidence-backed; release readiness is not a
  README assertion.
- A new operator can install, diagnose, upgrade, back up, and restore Clotho
  using published artifacts and documentation.
- An unfamiliar, least-privileged agent can orient, execute a bounded task,
  recover from interruption, and hand off through public surfaces only.
- New frontier work follows the core-scope decision rule in
  [`frontier-roadmap.md`](frontier-roadmap.md); integrations remain integrations
  when they do not strengthen Clotho's source-of-truth role.

### PRD v5 (Production platform + community trust)

- An organization is an enforced tenant boundary across database, storage,
  cache, queue, log, audit, provider, webhook, agent, and background-work paths.
- Hosted Clerk and production-self-host OIDC resolve into the same Clotho-owned
  memberships, roles, grants, policies, tokens, and audit model.
- Compose and Helm/Kubernetes profiles share source, schemas, migrations, REST
  contracts, and conformance tests; hosted convenience creates no proprietary
  control plane.
- Accepted work is durable before acknowledgement. Stateless edges and elastic
  workload cells scale independently from stateful VCS/database/storage owners.
- Tenant quotas, fair scheduling, metering, recovery, rolling upgrades, SLOs,
  and incident evidence precede production and autoscaling claims.
- Signals express permission-safe, self-declared repository interest. Following
  remains private; Signals grant no authority; Lachesis-derived adoption remains
  separately inspectable evidence.

---

## 7. Public interfaces

- **REST:** canonical public contract under `/api/v1/*`. Present today: orgs,
  users, activity, repos (tree/file/commits/oplog/submit), issues/PRs, branches,
  statuses, Actions, providers, secrets, agent-sessions. **Stage 15:** OpenAPI +
  fill gaps required for CLI/MCP parity. **Stage 16:** `/signals`, profiles.
  **Stages 17–20:** AuthProvider session/API-key verification; storage
  provider connect; repo `kind` + storage stats; Tailscale network connect +
  repo network policy; `/merge-queue` visibility; `/sandboxes` sessions;
  provenance on commits. `/sandboxes` only when CCI session APIs are honest.
- **Compute:** CCI multi-provider registry (Stage 12); secrets-bound credentials
  (Stage 13); honest Box + ComputeSDK completion (Stage 14); BYOC
  `clotho-runner` as a CCI provider (Stage 19).
- **Storage / network:** ObjectStoreProvider + NetworkProvider under the
  fabric (Stages 18–19); Arachne↔VCS pointer protocol (Stage 18).
- **Web:** settings and creation before social/discovery polish (Stage 13
  largely done; issue/PR depth still open). Add Clerk sign-in, provider
  fabric settings (storage/network), model/dataset overview, merge-queue and
  sandbox UX in Stages 17–20.
- **CLI/MCP:** API-backed wrappers only; no shelling out to `git`/`jj` from
  Clotho services. Stage 15 raises CLI/MCP to parity with stable REST; Stages
  17–20 extend the same rule to new routes.
- **Public-alpha contract:** Stage 22 freezes stable error, pagination,
  idempotency, async-operation, audit-correlation, CLI exit, and MCP capability
  semantics before the surface grows further.
- **Production foundation:** Stages 23–24 add tenant-safe identity/authorization,
  hosted and production self-host profiles, HA operations, quotas, metering,
  durable scheduling, and elastic workload cells.
- **Frontier:** Stages 25–29 add handoff/task, evidence/policy/Signals,
  compute-binding, lazy/projection, connector/lifecycle, and federation APIs
  through REST first.

---

## 8. Test plan

- Unit/integration tests for new Postgres control-plane tables and permission
  checks.
- Gateway tests for every new REST route and facade mapping.
- SDK tests for every public API method.
- Web typecheck/lint plus focused Playwright smoke tests for repo creation,
  settings, issue/PR workflows, Actions, provider pages, and agent pages.
- Compute provider contract tests using disabled/fake providers by default;
  env-gated live tests for Daytona, ComputeSDK bridge providers, and Box.
- MCP end-to-end tests for scoped agent permissions and parity workflows.
- Contract tests for stable error codes, pagination, idempotent replay,
  cancellation, conditional writes, request/audit correlation, OpenAPI↔SDK
  schema parity, CLI stdout/exit behavior, and MCP↔REST equivalence.
- Security tests for tenant isolation, SSRF, archive/path traversal, webhook
  replay, secret redaction, prompt-injection authority boundaries, and scoped
  agent capabilities.
- Migration and full backup/restore drills across Postgres, Git objects,
  Arachne/object storage, and the secrets key.
- Automated WCAG checks plus light/dark visual regression for every critical
  public-alpha journey.
- **v3 additions:** Clerk (or mock AuthProvider) session + API-key paths;
  ObjectStoreProvider probe/configured honesty; Arachne commit/fetch
  round-trip + dedup measurement; Tailscale connect mocked + env-gated live
  ephemeral join; merge-queue durability across restart; sandbox
  checkpoint/restore; provenance trailer assertions on agent commits.
- **v4 additions:** unfamiliar-agent handoff acceptance; capsule stale-base and
  scope tests; evidence-graph impact queries; release policy explanations;
  warm GPU binding provenance; lazy range-fetch correctness; cross-protocol
  digest identity; retention/deletion traceability.

---

## 9. Key risks & dependencies

| Risk | Notes |
|---|---|
| `jj-lib` is explicitly experimental/pre-1.0 | API churn is likely; pin a specific version and track upstream changes deliberately rather than auto-upgrading. |
| Forgejo GPLv3 boundary | Keep `collab/forgejo` strictly submoduled with patches tracked separately, not merged into Clotho's own crates/packages, to keep licensing clean until the org makes a deliberate decision (§11). |
| `xet-core` designed around HF's CAS service | Our S3/MinIO backend needs a compatible content-addressed-store shim; budget real time for this even though the chunking/xorb logic is reusable as-is. Stage 18 increases coupling: VCS pointer protocol must stay compatible. |
| Multi-agent merge-queue is genuinely unsolved territory | Stage 5's "naive-but-real" framing was deliberate. Stage 20 (ADR-0022) raises the bar to durable + speculative CI without claiming semantic merge yet. |
| Third-party "agentic-jujutsu"-style crates | Treat marketing claims (e.g., unverified performance multipliers) skeptically; fine as design inspiration, not as a dependency for the core engine. |
| SDK/API drift | Stage 15 must add OpenAPI generation or another single typed contract source before the surface area grows too large to manually keep aligned. |
| Secret handling expectations | Stage 13 shipped encrypted org/repo secrets (ADR-0014). Remaining risk: master-key bootstrap, rotation tooling, and provider-specific inject paths (Box/ComputeSDK in Stage 14; Tailscale OAuth + BYO S3 in Stages 18–19). |
| CLI/MCP lag | CLI and MCP still cover a fraction of REST; agent-native thesis fails if web-only features accumulate. Stage 15 is mandatory before discovery (Stage 16). |
| Stub honesty | Box and ComputeSDK must not stay “configured-looking” without runnable jobs (Stage 14). Same rule for Tailscale and BYO storage. |
| Clerk vs self-host purity | Managed Clerk must not block self-host: AuthProvider + later OIDC (ADR-0018). Document clearly what `just demo` needs vs managed deploy. |
| Tailscale customer ACL burden | v1 generates ACL snippets rather than mutating customer tailnets — UX must make copy-paste fail-safe. |
| Semantic merge competitors | Grit/dkod race on AST merge; Clotho defers symbol-aware merge until after Stage 20 runtime (ADR-0022 §4). |

---

## 10. Assumptions

- Forgejo remains unmodified and internal.
- Clotho-owned identity becomes the v2 source of truth; Forgejo identity is an
  internal/provider mapping only.
- Provider secrets: primary path is Clotho secrets store (ADR-0014); process
  env remains a local-dev escape hatch only.
- ComputeSDK is adopted through a bridge behind CCI, not by replacing the Rust
  compute boundary.
- API/SDK stability comes before CLI/MCP wrappers, but no feature is considered
  mature until web, SDK, CLI, and MCP coverage exists (Stage 15 acceptance).
- **v3:** Clerk is the default *managed* human AuthProvider; `bootstrap`
  remains valid for local/dev; agents never go through Clerk.
- **v3:** Capability-oriented scheduling is the UX default; explicit
  `provider_id` remains an escape hatch.
- **v3:** Tailscale is optional; public compute/storage demos must keep working
  without a customer tailnet.

---

## 11. Open decisions

1. **Clotho's own license** — MIT/Apache-2.0 (max adoption, permissive) vs. AGPLv3 (closes the SaaS loophole competitors could exploit, consistent with Forgejo's own GPLv3 stance) vs. a source-available/BSL model. This affects how tightly we can integrate Forgejo's GPLv3 code and what "open-source" means in the marketing page's promises.
2. **Fork Forgejo vs. stay API-level** — PRD v2 assumes we keep Forgejo internal and unmodified. Decide explicitly if deeper integration (for example surfacing jj's operation log inside Forgejo's own UI) is ever worth taking on GPLv3 obligations for that specific code.
3. **First external compute provider** — ~~Daytona is recommended for Stage 7 (persistent workspace, fast cold start, self-hosting story), but E2B (microVM isolation) is the safer pick if untrusted agent-generated code execution is a concern even in prototype form.~~ **Resolved (2026-07-07):** Daytona, integrated behind the Rust-native CCI so it stays swappable (docs/adr/0008, Stage 7 note). E2B remains a drop-in second `ComputeProvider` impl if microVM isolation is later wanted.
4. **Provider bridge deployment shape** — ~~Stage 12 needs a concrete choice for
   whether the ComputeSDK bridge runs as a small TypeScript sidecar, a separate
   service, or a constrained worker process behind `clotho-compute`.~~
   **Resolved (2026-07-08):** optional TypeScript HTTP sidecar
   (`services/compute-sdk-bridge`) behind CCI — not an in-process Node worker
   and not a public product boundary (docs/adr/0013). ComputeSDK packages and
   multi-provider routing stay inside the sidecar; Clotho depends only on CCI.
5. **Durable Actions store** — Stage 10 intentionally uses gateway-local state.
   Before Actions become product history, choose Postgres-in-gateway vs. a
   separate `clotho-actions` service. (Stage 11 already persists runs in
   gateway Postgres migrations; product-history design remains open.)
6. **Stage order after 13** — ~~Recommended default: **Stage 15 (developer
   surfaces) in parallel with or immediately after Stage 14 (honest compute)**.
   Do **not** start Stage 16 discovery until Stage 15 acceptance is met.
   Stage 13 remaining web polish (issue/PR depth) can interleave but must not
   ship web-only APIs without SDK methods.~~ **Amended (2026-07-09, PRD v3):**
   finish Stage 14/15 honesty and parity, then prefer **Stages 17→20** (auth
   fabric, Arachne↔VCS, Tailscale/BYOC, agent runtime) before Stage 16/21
   discovery. Issue/PR web depth may interleave but must not ship web-only.
7. **Human API keys under Clerk** — Keep minting Clotho `clotho_tok_…` human
   tokens alongside Clerk org API keys, or fully delegate human machine auth
   to Clerk? (ADR-0018 leaves this open at implementation time.)
8. **CI large-file materialization** — When Arachne pointers land in trees,
   should ExportRepoArchive resolve blobs into the sandbox tarball, or should
   sandboxes fetch pointers with Clotho credentials? (ADR-0020.)

---

## 12. Historical first tickets

1. `chore(scaffold)`: create the full monorepo directory structure from §3, with placeholder `README.md` in every top-level directory explaining its purpose.
2. `chore(rust)`: initialize the Cargo workspace with all seven crates from §3 as empty lib/bin crates, each exposing a trivial gRPC health-check endpoint (`clotho-common` first, since others will depend on it for shared types).
3. `chore(js)`: initialize the pnpm workspace with `apps/web`, `apps/site`, `packages/ui`, `packages/sdk-js`, `packages/config`; port the existing teaser page's design tokens (colors, type scale from the landing page CSS) into `packages/ui` as a starting design system.
4. `chore(infra)`: write `docker-compose.dev.yml` with Postgres, MinIO, and stub containers for each Rust service; confirm `docker compose up` succeeds from a clean clone.
5. `docs(adr)`: write ADR-0001 formalizing the jj-lib decision, and ADR-0002 formalizing the xet-core decision, using §2's rationale as a starting point.
6. `feat(clotho-vcs)`: add `jj-lib` as a dependency, implement `init_repo` and `commit` as the first two gRPC methods, with one integration test proving a real commit lands in a real git object.

---

*This is a living document. Update stage exit conditions as reality corrects the plan — don't let the plan silently drift from what's actually built.*
