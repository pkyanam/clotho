# ADR-0022: Agent runtime v2 — durable merge-queue, sandboxes, provenance

- **Status:** Accepted
- **Date:** 2026-07-09
- **Deciders:** Clotho core
- **Related:** ADR-0005 (agent identity), ADR-0006 (naive merge-queue),
  ADR-0009 (MCP writes), ADR-0017 (merge policy), vision §7

## Context

ADR-0006 shipped a deliberate naive merge-queue: write-time non-blocking,
land-time serialized via in-process mutex, conflict commits land on `main`.
That proved the concept (Stage 5) but is not a platform:

- No durable queue across restarts
- No speculative CI before land
- No per-agent workspace productization (engine is workspace-less tree ops)
- No `/sandboxes` session API (Box persistent hooks exist but unused)
- Provenance is audit-log + agent-named commits — not an attestable standard

The Dream Roadmap's haunting failure **B** is exactly this gap: agents and
humans host work on Clotho, but multi-agent landing and compute sessions
still feel bolted on. Competitors (Grit, dkod, Gas Town, Colony) are racing
on orchestration and semantic merge; Clotho must own the **forge-native**
landing strip.

## Decision

### 1. Merge-queue v2 (durable + speculative)

Replace the in-process-only queue with a persisted control-plane queue:

| Concern | v1 (ADR-0006) | v2 |
|---|---|---|
| Ordering | Per-repo async mutex | Per-repo durable queue (Postgres) |
| Restart | In-flight lost / unclear | Submissions survive; resume or fail cleanly |
| Conflicts | Conflict commit lands | Same jj first-class conflicts + clearer API/UI |
| CI | Push webhook after land | **Speculative CI** on the integration candidate before advancing `main` (configurable) |
| API | `SubmitChange` gRPC | Same RPC + REST `/merge-queue` visibility (entries, status, cancel) |

Engine still owns mutation (`IntegrateCommit`); queue owns ordering and
policy gates (ADR-0017 merge policy + required Actions).

Non-goals for v2: perfect multi-writer working copies; full semantic/AST
merge (see §4).

### 2. Sandbox sessions API

Expose long-lived agent/human workspaces as a public product:

```text
POST   /api/v1/sandboxes          # create (provider, repo, capabilities)
GET    /api/v1/sandboxes/{id}
POST   /api/v1/sandboxes/{id}/exec
POST   /api/v1/sandboxes/{id}/checkpoint   # ties to jj checkpoint
POST   /api/v1/sandboxes/{id}/restore
DELETE /api/v1/sandboxes/{id}     # stop / tear down
```

- Backed by CCI session capabilities (Box persistent path first; Daytona
  where persistent; BYOC runners when registered).
- MCP tools wrap the same REST edge (Stage 15 rule) — e.g. `sandbox_create`,
  `sandbox_exec`, `checkpoint` already exists at VCS layer and should link
  to session id when present.
- NetworkProvider (ADR-0021): sandboxes may join the org tailnet when
  `private-net` is required.

Do not ship `/sandboxes` until at least one provider can honestly run a
persistent or resumable session (Stage 14 honesty rule).

### 3. Provenance standard (Clotho trailers + audit)

Every agent-authored commit SHOULD carry structured provenance:

```text
Clotho-Agent: <agent_id>
Clotho-Agent-Name: <name>
Clotho-Run: <action_run_id or sandbox_id>
Clotho-Session: <mcp_session_or_token_id>
Clotho-Prompt-Sha256: <optional hex>
```

- Stored as git commit trailers (and mirrored in activity/audit tables).
- Human UI and MCP `orient` / log views surface provenance without scraping.
- Merge policy (ADR-0017) may require human review for commits with
  `Clotho-Agent` trailers (org/repo setting).
- Signing/attestation (sigstore-style) is a follow-on; trailers + audit are
  the v1 standard Clotho can propose externally.

### 4. Symbol-aware merge (explicitly later)

`clotho-diff` already provides tree-sitter symbol diffs for humans and
agents. **Semantic merge** (AST-level auto-merge of non-overlapping
symbols) is valuable and contested (Grit/dkod). Schedule it **after**
durable queue + sandboxes + provenance so Clotho merges *and* hosts,
rather than chasing merge science before the forge runtime is solid.

### 5. Parity

- REST is canonical; CLI `clotho merge-queue|sandbox …`; SDK types; MCP tools
  through the edge.
- OpenAPI updated in the same change set.
- Tests: queue durability across gateway restart; speculative CI failure
  blocks land; sandbox checkpoint/restore round-trip; provenance trailers
  present on MCP `commit`.

## Consequences

- ADR-0006 remains historically correct for the prototype; new work
  implements v2 rather than silently expanding the mutex.
- Actions control plane and merge-queue become tightly coupled for
  speculative CI — shared run records, clear status on queue entries.
- Agent identity (ADR-0005) gains a clearer "session" notion linking token
  audit rows to sandbox/run ids (schema additive migration).
- Avoids failure B: multi-agent landing and compute sessions become
  first-class product surfaces, not demo scripts over raw gRPC.
