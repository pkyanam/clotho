# Clotho
### A version control & collaboration platform for humans and AI agents
**Master Vision & Architecture Spec — v0.2 (July 2026)**

---

## 1. Vision

GitHub was built for a world where every commit came from a human typing at a keyboard. That world is over. In 2026, a meaningful and growing share of commits, PRs, and reviews are produced by AI agents — sometimes dozens working in parallel on the same repository — and the tools underneath them (git's staging area, file-locking assumptions, linear review queues) were never designed for that.

**Clotho is the version control platform built for the world as it actually is now: humans and agents, working together, on the same repo, at the same time.**

The repository is the product's unit of truth, but “repository” means more than
a Git tree. A Clotho repository binds source, models, datasets, evaluations,
releases, agent work, compute, network reach, storage placement, policy, and
provenance to one recoverable history. Other systems may execute or mirror that
state; Clotho remains the place where identity, intent, evidence, and the final
artifact meet.

Design targets, stated plainly:

- **As simple as Vercel** — zero-config to first deploy/first commit, opinionated defaults, delightful UI.
- **As powerful and robust as Cloudflare** — a real platform with a serious edge network and primitives underneath the simple surface, not a toy.
- **Ultra-performant** — sub-second clone/push/pull even for multi-gigabyte repos, chunk-level dedup, global edge caching.
- **Open, self-hostable, and modular** — every major subsystem (compute, storage, database, network) should be swappable. No lock-in, by design, as a competitive stance against GitHub/GitLab.
- **Agent-native, not agent-adjacent** — agents are first-class identities with their own permissions, checkpoints, and structured APIs, not humans-with-a-bot-flag bolted onto a 2008-era data model.
- **Verifiable by default** — a release is not merely a tag; it is a commit,
  artifact manifest, evidence graph, policy decision, and immutable digest.
- **Protocol-friendly** — teams should be able to adopt Clotho without replacing
  every client immediately. Git, Hugging Face, OCI, and artifact protocols can
  project one Clotho-owned release without becoming competing sources of truth.

---

## 2. Naming

| Name | Role | Why |
|---|---|---|
| **Clotho** | The platform | The Fate who *spins* the thread of life onto the spindle — the one who creates, not measures or cuts. Maps naturally to a commit graph that's continuously spun forward by many hands (and non-hands) at once. |
| **Arachne Engine** | The storage/dedup subsystem | Weaves thousands of content-addressed chunks back into exact files, fast enough to rival the gods (Xet-style chunk dedup). Named for supreme technical skill at weaving — the appropriately dark subtext (hubris) lives comfortably at the infra layer, out of the spotlight. |
| *(reserved)* **Lachesis** | Possible future name for the metrics/observability/diff-measurement layer — the Fate who measures the thread. |
| *(reserved)* **Atropos** | Possible future name for the retention/garbage-collection/deletion layer — the Fate who cuts the thread. |

---

## 3. System architecture — four decoupled layers

The single biggest architectural decision: **don't build a monolith.** GitHub and GitLab conflate version control, storage, collaboration, and compute into one inseparable stack. Clotho treats each as a layer with a stable interface, so any layer can be swapped by an operator without forking the whole platform.

```
┌─────────────────────────────────────────────────────────┐
│  Frontend / Collaboration Layer  (the "GitHub" surface)  │
│  issues · PRs/reviews · orgs & permissions · web UI       │
└─────────────────────────────────────────────────────────┘
┌─────────────────────────────────────────────────────────┐
│  Agent Interface Layer  (MCP server, structured diff API) │
└─────────────────────────────────────────────────────────┘
┌───────────────────────┬─────────────────────────────────┐
│  VCS Engine            │  Arachne Storage Engine          │
│  (jj-based, git-native)│  (Xet-protocol chunk storage)    │
└───────────────────────┴─────────────────────────────────┘
┌─────────────────────────────────────────────────────────┐
│  Pluggable Infra Layer: compute · database · network      │
└─────────────────────────────────────────────────────────┘
```

### 3.1 VCS engine — Jujutsu-native, git-compatible

Build the engine on **Jujutsu (jj)** rather than raw git plumbing. Rationale:

- **Operation log as a first-class API.** jj records every operation performed on the repository — commits, pulls, pushes, rebases, undos — as a queryable log. This is exactly the primitive a supervising system needs to manage an agent's session safely: always a reliable path back to a known-good state, exposed as an API call (`checkpoint`, `restore_to`) rather than a CLI trick.
- **No staging area, working-copy-as-commit.** Removes an entire class of "agent forgot to `git add`" failures.
- **Conflicts as first-class, non-blocking objects.** A rebase through a conflict doesn't stop — the commit lands marked conflicted and gets resolved later. This matters enormously once N agents are committing concurrently; you don't want them deadlocked waiting on each other.
- **Git-compatible storage backend.** jj reads/writes real `.git` directories by default, so every commit Clotho produces is a real git commit — existing git tooling, CI systems, and human muscle memory keep working unmodified.

**Open problem we own, not inherit:** jj's working-copy model is single-writer per workspace. Real-world testing shows two agents in one workspace can cause one agent's commit to absorb another's changes. Clotho's answer: **one jj workspace per agent by default**, orchestrated by Clotho's scheduler, with a merge-queue service that reconciles workspaces back into the shared graph — this is genuinely unsolved industry-wide as of mid-2026, and is where Clotho does real novel engineering rather than integration work.

### 3.2 Arachne Engine — storage, modeled on Xet

Model this directly on Hugging Face's Xet protocol, which is an open, implementation-agnostic spec (not just an HF-internal system):

- **Content-defined chunking** (GearHash rolling hash, ~64KiB average chunk size, 8–128KiB bounds) instead of fixed-size blocks — inserting or deleting bytes only affects nearby chunks, so dedup survives edits, unlike git LFS's file-level dedup.
- **Xorbs**: chunks batched into ~64MiB immutable containers, so a multi-GB model checkpoint doesn't turn into tens of thousands of individual HTTP requests or S3 objects.
- **Global, permission-aware dedup**: a tokenizer config identical across 100 forked models is physically stored once, with access enforced at the chunk level so you only ever read chunks you're authorized to reconstruct.
- **Protocol-first, not vendor-first**: because the spec is published and interop-tested, Clotho can implement a compatible server without depending on HF's infrastructure — and, longer-term, interop *with* HF/Xet-based storage becomes possible rather than requiring migration.
- **Backward-compat bridge**: speak git-LFS pointer format at the edges so existing tooling (and existing LFS repos people want to import) keeps working during migration.

This is what makes Clotho legitimately better than GitHub+LFS for anything ML/data-heavy — model weights, datasets, design files, video — without asking users to think about "which storage system is this file in."

### 3.3 Collaboration layer

Don't reinvent mature collaboration plumbing. Run **Forgejo unmodified as an
internal provider** for Git HTTP and selected issue/PR compatibility while
Clotho owns the public web, REST, SDK, CLI, MCP, identity, policy, audit, and
product semantics. This preserves a clean GPL boundary and lets Forgejo be
replaced without changing the user-facing contract. Spend novelty budget on
the VCS, storage, agent runtime, evidence, and provider fabric—not on exposing a
second forge UI.

### 3.4 Agent interface layer — the actual differentiator

This is the layer nobody has shipped a canonical answer for yet, which is exactly why it's worth building:

- **Native MCP server**, not a bolt-on integration — `checkpoint`, `diff_symbol`, `orient_repo`, `open_pr`, `request_review`, `list_agent_sessions` as tool calls agents use directly.
- **Structured diffs**: a tree-sitter-backed symbol/AST-level change API alongside raw text diffs, so an agent can ask "what changed in function X" instead of parsing patch text.
- **Non-human identity as a primitive**: agents get scoped, revocable, individually-audited credentials — distinct from OAuth apps or service accounts — with per-action rate limits and full provenance (which agent, which run, which prompt/session produced this commit). This is the piece that makes "replace GitHub for agent-heavy teams" credible instead of aspirational.
- **Review UX that works for both audiences**: a PR view that's equally legible to a human skimming it and to another agent consuming it programmatically (i.e., the same underlying structured-diff object powers both renderings).

---

## 4. Pluggable infrastructure layer (your addition — this is the right call)

This is where Clotho earns the "as robust as Cloudflare, as simple as Vercel" positioning: powerful primitives, but the operator (or even individual user) picks the backend.

### 4.1 Compute / CI / runners — provider-agnostic by design

Don't build a proprietary runner fleet as the *only* option. Define a **Clotho Compute Interface (CCI)** — a thin abstraction similar in spirit to what **ComputeSDK** has already proven out in the sandbox-provider space (they support unified access to E2B, Daytona, Modal, Vercel, Railway, Render, Blaxel, Namespace, and BYOC infra behind one interface, with a 2.0 "Sandbox Gateway" that's fully BYOK). Clotho should either:
- adopt ComputeSDK directly as the abstraction layer for agent-execution sandboxes, or
- define a compatible interface so anything that speaks ComputeSDK's provider model works with Clotho with minimal glue.

Backends to support day one or shortly after:
- **Hyperscalers**: AWS, GCP, Azure (self-hosted runners on your own account/VPC)
- **Agent-sandbox specialists**: Daytona (persistent workspace, container isolation, fast cold start), E2B (Firecracker microVM, hardware-level isolation), Modal
- **Bring-your-own-device**: a lightweight runner agent (think self-hosted GitHub Actions runner) so individuals/hobbyists can donate a spare machine or use their own laptop/homelab with zero cloud spend
- **ComputeSDK-compatible gateway** as the universal adapter, so new providers Clotho hasn't explicitly integrated still work

Practical implication: CI/CD config in Clotho should specify *what kind of isolation and persistence it needs* (untrusted code → microVM; long-lived dev environment → persistent container; GPU job → provider with GPU support) rather than hard-coding a provider, and let the CCI resolve that to whatever's configured.

### 4.2 Private networking — Tailscale integration

This is a genuinely good idea and underused in this category. Concretely:

- **Tailscale (or generic WireGuard-based tailnet) as a first-class network target** for runners: a self-hosted runner or agent sandbox joins the user's tailnet automatically, so CI jobs can reach internal services (private databases, internal APIs, on-prem GPUs) without exposing anything publicly or requiring VPN gymnastics.
- **"Private cloud" mode**: an org can run 100% of Clotho's compute (and optionally storage) inside their own tailnet — Clotho's control plane orchestrates, but no code or data ever leaves their network boundary. This is a strong enterprise/regulated-industry pitch (finance, healthcare, defense) and a strong "true self-hosting" pitch for privacy-conscious individuals.
- Tailscale's ACL/tag model maps cleanly onto Clotho's agent-identity model — an agent's runner can be tagged and scoped in the tailnet the same way its Clotho credentials are scoped, giving one coherent permission story across network and platform layers.

### 4.3 Data layer — extensible database backends

Two distinct things worth separating:

1. **Clotho's own control-plane database** (metadata: repos, PRs, permissions, agent sessions) — should support pluggable backends itself (Postgres primary, with SQLite for single-node/self-host-lite, and a documented adapter interface for others).
2. **User-facing "attach your own database" extensibility** — this is the more interesting idea you raised. Concretely: a **connector/adapter framework** (à la how MCP itself works, or dbt's adapter model) that lets a repo declare a bound external data source — Postgres, MySQL, ClickHouse, a vector DB, a data warehouse — and exposes it through Clotho's agent interface layer so agents working in that repo can query schema/sample data as *repository context* without Clotho ever owning or storing that data. This turns Clotho from "stores your code and models" into "understands the data your code and agents actually operate on," which is a meaningfully different value prop from GitHub.

---

## 5. Product experience targets

- **"Vercel-simple"**: `clotho init` → repo live, agent-ready, CI configured with sane defaults, in under a minute. No YAML spelunking required for the common path.
- **"Cloudflare-robust"**: the simple path is a thin layer over real primitives (CCI, Arachne, tailnet integration) that power users and enterprises can reach into directly — nothing is a black box you outgrow.
- **Beautiful frontend**: fast, keyboard-driven, real-time collaborative (multiple humans *and* agents visibly working on a repo at once, presence indicators for agent sessions the way Figma shows cursors).
- **Performance bar**: clone/checkout latency and push/pull throughput should be benchmarked against both git+LFS and HF Hub/Xet on equivalent large-file workloads, published openly — performance-as-marketing, the way Vercel and Cloudflare do.

---

## 6. Phased roadmap

**Phase 0 — Foundations (proof of concept)**
- Fork/stand up Forgejo as collaboration shell
- Implement Arachne storage engine against the published Xet protocol spec, S3-compatible backend
- jj-as-engine with git bridge; validate round-trip compatibility with existing GitHub/GitLab workflows

**Phase 1 — Agent-native surface**
- MCP server (checkpoint/restore, structured diff, orient)
- Non-human identity & credential model
- Single-workspace-per-agent orchestration + merge-queue reconciliation

**Phase 2 — Pluggable infrastructure**
- CCI abstraction; integrate ComputeSDK or ship a compatible interface
- BYO-device runner
- Tailscale/tailnet integration for private-cloud mode
- *Productization (PRD v3 / ADRs 0018–0021):* AuthProvider (Clerk-first), Provider Fabric
  (BYO object store + NetworkProvider), Arachne on the VCS path, Tailscale private-reach
  and `clotho-runner` BYOC — see docs/prd.md Stages 17–19.

**Phase 3 — Data layer extensibility**
- External database connector framework
- Agent-queryable repo-bound data context

**Phase 4 — Scale & polish**
- Global edge caching for Arachne chunks/xorbs
- Federation (ForgeFed-compatible)
- Published performance benchmarks vs. GitHub+LFS and HF Hub
- *Agent runtime v2 (PRD v3 / ADR-0022):* durable merge-queue, sandbox sessions, provenance
  trailers before discovery/social polish (prd Stages 20–21).

**Phase 5 — Verifiable autonomous development**
- Public interface, security, recovery, accessibility, and packaging hardening
- Versioned Handoff Capsules and an executable repository task plane
- Lachesis evidence graph plus explainable release/evaluation policy
- Capability-based compute bindings with warm GPU and data-local execution
- Lazy virtual repositories and multi-protocol release projection
- Open provider/connector conformance kit and Atropos lifecycle policy

See [`release-readiness.md`](release-readiness.md) for the immediate gate and
[`frontier-roadmap.md`](frontier-roadmap.md) for the sequenced product thesis.

---

## 7. Open problems (where the real work is)

These are not solved anywhere in the industry as of mid-2026 — this is the actual moat, not the integration work:

1. **Concurrent multi-agent writes to one workspace.** jj's single-writer model needs an orchestration layer above it; nobody has shipped this well yet.
2. **Agent identity & provenance standards.** No consensus format exists yet for "which agent, which run, which prompt lineage produced this commit" — Clotho could help define this.
3. **Structured diff format that satisfies both humans and agents equally well** — most tools optimize for one audience.
4. **Trust/verification of agent-authored commits** at scale — signing, attestation, and review-routing policy for machine-authored changes is still an open design space.

---

*This is a living spec — intended as a starting point for architecture review, not a final design.*
