# Clotho — Product Requirements
### Prototype record and v2 modular-platform roadmap
**v0.2 — July 2026**

---

## 0. Purpose of this document

This PRD started as the scope for a **working internal prototype**. Stages 0-10 record the path from scaffold to jj-backed VCS, Arachne storage, Forgejo facade, MCP agents, CLI, Actions, Daytona compute, and a basic web shell.

PRD v2 shifts the product from "prove the stack works" to "make Clotho a mature modular platform." The default direction is:

- Keep Forgejo internal, unmodified, and behind Clotho-owned API/web surfaces.
- Keep the Rust-native CCI as Clotho's compute contract.
- Add ComputeSDK as an optional bridge/adapter behind CCI, not as the core product boundary.
- Add Box later as a richer persistent VM / agent-workspace provider, not as a replacement for short-lived CI sandboxes.
- Treat web, REST, SDK, CLI, and MCP parity as part of feature maturity, not separate polish.

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
  `computeProviderList` / richer `ComputeProvider`. Secrets remain env-backed.
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

### Stage 14 — World-Class API / CLI / SDK / MCP Parity
- Make REST the canonical public API first; every stable web feature must have
  an SDK method.
- Add OpenAPI generation or a single typed contract source so SDK drift stops.
- Expand CLI to cover repos, issues, PRs, Actions, agents, provider status,
  settings read/write, logs, and activity.
- Expand MCP tools for agents: list/create issues, comment/review PRs, run
  actions, fetch logs, inspect settings, list providers, create sandbox
  sessions, and query activity/provenance.
- **Acceptance:** an AI agent can operate Clotho through MCP with the same core
  capabilities a human has in the web app.

### Stage 15 — Discovery, Social, and Competitive UX
- Add Clotho's GitHub Stars competitor as `Signals`: users/orgs can signal
  repos, optionally categorize them, and use signals for discovery/ranking.
- Add repo/user/org profiles, public/private visibility, trending repos,
  activity timelines, contribution/commit leaderboards, agent leaderboards, and
  "most active / most reliable / most reviewed" views.
- Add commit-history leaderboards built from Clotho's own commit/provenance
  data, not scraped UI state.
- **Acceptance:** Clotho has a credible discovery/community layer while
  preserving enterprise/self-host privacy controls.

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

---

## 7. Public interfaces

- **REST:** add `/api/v1/orgs`, `/api/v1/users`, `/api/v1/settings`,
  `/api/v1/activity`, `/api/v1/providers`, `/api/v1/sandboxes`,
  `/api/v1/repos/{repo}/signals`, and expanded repo issue/PR/settings routes.
- **Compute:** evolve CCI into provider lifecycle plus job/session APIs; keep
  one-shot Actions compatible.
- **Web:** prioritize settings and creation flows before social/discovery
  polish.
- **CLI/MCP:** API-backed wrappers only; no shelling out to `git`/`jj` from
  Clotho services.

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

---

## 9. Key risks & dependencies

| Risk | Notes |
|---|---|
| `jj-lib` is explicitly experimental/pre-1.0 | API churn is likely; pin a specific version and track upstream changes deliberately rather than auto-upgrading. |
| Forgejo GPLv3 boundary | Keep `collab/forgejo` strictly submoduled with patches tracked separately, not merged into Clotho's own crates/packages, to keep licensing clean until the org makes a deliberate decision (§11). |
| `xet-core` designed around HF's CAS service | Our S3/MinIO backend needs a compatible content-addressed-store shim; budget real time for this even though the chunking/xorb logic is reusable as-is. |
| Multi-agent merge-queue is genuinely unsolved territory | Stage 5's "naive-but-real" framing is deliberate — do not let this stage's scope creep into solving it perfectly; the prototype needs *a* working answer, not *the* answer. |
| Third-party "agentic-jujutsu"-style crates | Treat marketing claims (e.g., unverified performance multipliers) skeptically; fine as design inspiration, not as a dependency for the core engine. |
| SDK/API drift | Stage 14 must add OpenAPI generation or another single typed contract source before the surface area grows too large to manually keep aligned. |
| Secret handling expectations | Provider settings may show configured/masked state, but secrets remain environment-backed until encryption and key-management are explicitly designed. |

---

## 10. Assumptions

- Forgejo remains unmodified and internal.
- Clotho-owned identity becomes the v2 source of truth; Forgejo identity is an
  internal/provider mapping only.
- Provider secrets stay environment-backed until explicit encryption/key
  management is designed.
- ComputeSDK is adopted through a bridge behind CCI, not by replacing the Rust
  compute boundary.
- API/SDK stability comes before CLI/MCP wrappers, but no feature is considered
  mature until web, SDK, CLI, and MCP coverage exists.

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
