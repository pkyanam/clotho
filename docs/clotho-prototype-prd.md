# Clotho — Prototype PRD
### Internal working prototype: scope, tech stack, monorepo structure, development stages
**v0.1 — July 2026**

---

## 0. Purpose of this document

This PRD scopes a **working internal prototype**, not the full platform from the vision spec. The goal is the smallest end-to-end slice that proves the hardest architectural bets — jj-native version control, Xet-style storage dedup, and an agent-native interface — actually fit together, before investing in the collaboration UI, federation, or managed hosting.

This doc is written to be handed directly to a coding agent today. Section 9 is the literal first set of tickets.

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
| Collaboration shell | **Forgejo (Go), run largely as-is, PostgreSQL-backed** | Forgejo is a mature, actively maintained, non-profit-governed Gitea fork with issues, PRs, org/permissions, and a documented Postgres schema. For the prototype, don't attempt deep jj-native UI integration inside Forgejo — point it at the same git-compatible objects the VCS engine writes, and treat Forgejo purely as the collaboration/issue/PR chrome. **Licensing flag:** Forgejo ≥v9.0 is GPLv3. If we fork and modify Forgejo's own source, GPLv3's copyleft applies to that code specifically (running it as SaaS without distributing modified source is fine under GPLv3, unlike AGPL — but any modified Forgejo code we *do* distribute must stay GPLv3). This is why Forgejo is walled off in its own `collab/` directory rather than mixed into Clotho's own codebase — see §5 and the open decision in §8. |
| Agent interface | **MCP server, Rust** (`clotho-agent-gateway`) | Exposes `checkpoint`, `restore_to`, `diff_symbol`, `orient_repo` as MCP tools backed directly by the VCS engine's gRPC API. Note: a third-party crate (`agentic-jujutsu`) already explores this space with WASM bindings and MCP transport — worth reading for interface ideas, but its production-readiness claims are unverified and it's not an org we have visibility into, so we build our own gateway directly on `jj-lib` rather than depending on it for the core engine. |
| Agent identity | Scoped tokens in Postgres, distinct table/model from human OAuth identities, per-action audit log | Matches the vision spec's non-human-identity requirement from day one, even in prototype form — retrofitting identity models later is painful. |
| Structured diff | Rust, `tree-sitter` for symbol-level parsing | Feeds both the human PR view and the agent-facing diff API from one object. |
| Compute abstraction | **ComputeSDK** (or a compatible adapter we write) for the CCI (Clotho Compute Interface) | Already proven multi-provider abstraction (E2B, Daytona, Modal, Vercel, Railway, Render, BYOC) with a BYOK gateway — don't rebuild this for the prototype, integrate it and prove one provider end-to-end (Daytona is a reasonable first pick given persistent-workspace + fast cold start). |
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
├── LICENSE                        # top-level Clotho license (decision needed — see §8)
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

### Stage 2 — Arachne storage engine (Week 2–4, parallel with Stage 1)
- Embed `xet-core` crates in `clotho-storage`; implement upload (chunk → xorb → S3 write) and download (reconstruct file from xorb + chunk ranges) against MinIO.
- **Exit condition:** upload a multi-GB synthetic file, then upload a near-duplicate (small modification) — confirm via storage metrics that only the changed chunks were newly written, and that download reconstructs both files byte-identical to source.

### Stage 3 — Collaboration shell integration (Week 3–5, parallel)
- Stand up Forgejo via the `collab/forgejo` submodule + Docker Compose.
- Point Forgejo at git-compatible repos managed by `clotho-vcs` (Forgejo talks to them as ordinary git repos — no Forgejo code changes needed for this prototype stage).
- Wire repo creation in the API gateway to provision both a Forgejo repo entry and a `clotho-vcs` repo.
- **Exit condition:** creating a repo through Clotho's API produces a real Forgejo project with working issues/PRs, backed by a jj-managed git repo.

### Stage 4 — Agent interface layer (Week 4–6)
- Build `clotho-agent-gateway` as an MCP server exposing `checkpoint`, `restore_to`, `diff_symbol`, `orient_repo`, backed by `clotho-vcs` and `clotho-diff`.
- Implement agent identity: scoped tokens, Postgres-backed, distinct from human OAuth, with per-call audit logging.
- **Exit condition:** a real MCP client (e.g., an agent session in a coding tool) can authenticate as a scoped agent identity, checkpoint work, intentionally break something, and restore — entirely through MCP tool calls.

### Stage 5 — Merge-queue prototype (Week 5–7)
- Naive-but-real version of multi-workspace reconciliation: two agent workspaces produce concurrent commits; the merge-queue serializes and rebases them, surfacing unresolvable conflicts as first-class jj conflict objects rather than blocking.
- **Exit condition:** two simulated agents committing concurrently to the same repo end up reconciled into one graph without a human intervening, for the non-conflicting case; conflicting case produces a clearly surfaced conflict commit.

### Stage 6 — Frontend (Week 5–7, parallel)
- Evolve `apps/site` (teaser) design tokens into `packages/ui`.
- Build `apps/web`: repo browser, PR view (proxying Forgejo), basic agent-session presence panel (can be mocked/polling for the prototype, no need for real-time infra yet).
- **Exit condition:** a human can browse a repo, view a PR with a structured diff, and see which agent sessions have touched it recently.

### Stage 7 — Pluggable compute + integration demo (Week 7–8)
- Integrate ComputeSDK (or a minimal compatible adapter) for one provider (Daytona recommended first).
- Wire a push webhook → CCI → sandbox run → status reported back to the PR.
- Run the full definition-of-done scenario from §1 end to end.
- **Exit condition:** the Stage 7 demo scenario runs live, recorded, and reproducible from a clean `docker compose up`.

---

## 6. Success criteria for the prototype (overall)

- All Stage 0–7 exit conditions met.
- No component depends on shelling out to the `jj` or `git` CLI binaries at runtime — everything goes through `jj-lib`/`gitoxide` embedded APIs.
- Storage dedup is *measured*, not assumed — the demo must show a real before/after byte count.
- At least one agent identity is fully distinct from a human identity in the data model, not a flag on a user row.
- The whole stack runs from a single `docker compose up` on a laptop.

---

## 7. Key risks & dependencies

| Risk | Notes |
|---|---|
| `jj-lib` is explicitly experimental/pre-1.0 | API churn is likely; pin a specific version and track upstream changes deliberately rather than auto-upgrading. |
| Forgejo GPLv3 boundary | Keep `collab/forgejo` strictly submoduled with patches tracked separately, not merged into Clotho's own crates/packages, to keep licensing clean until the org makes a deliberate decision (§8). |
| `xet-core` designed around HF's CAS service | Our S3/MinIO backend needs a compatible content-addressed-store shim; budget real time for this even though the chunking/xorb logic is reusable as-is. |
| Multi-agent merge-queue is genuinely unsolved territory | Stage 5's "naive-but-real" framing is deliberate — do not let this stage's scope creep into solving it perfectly; the prototype needs *a* working answer, not *the* answer. |
| Third-party "agentic-jujutsu"-style crates | Treat marketing claims (e.g., unverified performance multipliers) skeptically; fine as design inspiration, not as a dependency for the core engine. |

---

## 8. Open decisions (need a human call before/during Stage 3)

1. **Clotho's own license** — MIT/Apache-2.0 (max adoption, permissive) vs. AGPLv3 (closes the SaaS loophole competitors could exploit, consistent with Forgejo's own GPLv3 stance) vs. a source-available/BSL model. This affects how tightly we can integrate Forgejo's GPLv3 code and what "open-source" means in the marketing page's promises.
2. **Fork Forgejo vs. stay API-level** — the prototype plan assumes we don't modify Forgejo source at all in Stage 3. Decide before Stage 3 whether deeper integration (e.g., surfacing jj's operation log inside Forgejo's own UI) is worth taking on GPLv3 obligations for that specific code.
3. **First external compute provider** — Daytona is recommended for Stage 7 (persistent workspace, fast cold start, self-hosting story), but E2B (microVM isolation) is the safer pick if untrusted agent-generated code execution is a concern even in prototype form.

---

## 9. First tickets — hand these to an agent today

1. `chore(scaffold)`: create the full monorepo directory structure from §3, with placeholder `README.md` in every top-level directory explaining its purpose.
2. `chore(rust)`: initialize the Cargo workspace with all seven crates from §3 as empty lib/bin crates, each exposing a trivial gRPC health-check endpoint (`clotho-common` first, since others will depend on it for shared types).
3. `chore(js)`: initialize the pnpm workspace with `apps/web`, `apps/site`, `packages/ui`, `packages/sdk-js`, `packages/config`; port the existing teaser page's design tokens (colors, type scale from the landing page CSS) into `packages/ui` as a starting design system.
4. `chore(infra)`: write `docker-compose.dev.yml` with Postgres, MinIO, and stub containers for each Rust service; confirm `docker compose up` succeeds from a clean clone.
5. `docs(adr)`: write ADR-0001 formalizing the jj-lib decision, and ADR-0002 formalizing the xet-core decision, using §2's rationale as a starting point.
6. `feat(clotho-vcs)`: add `jj-lib` as a dependency, implement `init_repo` and `commit` as the first two gRPC methods, with one integration test proving a real commit lands in a real git object.

---

*This is a living document. Update stage exit conditions as reality corrects the plan — don't let the plan silently drift from what's actually built.*
