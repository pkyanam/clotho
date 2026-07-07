# ADR-0005: Agent identity — dedicated tables, scoped hashed tokens, per-call audit

- **Status:** Accepted
- **Date:** 2026-07-07
- **Deciders:** Clotho core

## Context

The vision spec (§3.4) makes non-human identity a primitive: agents get
scoped, revocable, individually-audited credentials — not OAuth apps, not
service accounts, not a `is_bot` flag on a user row. The PRD (§2, §6)
requires this from day one because retrofitting identity models is
painful, and success criterion §6 demands "at least one agent identity
fully distinct from a human identity in the data model".

Human identity, meanwhile, lives in the collaboration shell for the
prototype (Forgejo users, its own `forgejo` database). Clotho's own
control-plane database (`clotho` in the dev stack's Postgres) had no
schema until now.

## Decision

Three tables in the control-plane Postgres, owned by `clotho-agent-gateway`
(sqlx, embedded migrations in `crates/clotho-agent-gateway/migrations/`):

- **`agents`** — the identity itself: name, description, `disabled_at` as a
  kill switch that severs every credential at once. No password, no email,
  no OAuth linkage: an agent *is not a user*, and there is deliberately no
  foreign key anywhere toward human identity.
- **`agent_tokens`** — scoped bearer credentials. Only the SHA-256 of the
  token is stored; the plaintext (`clotho_agt_<64 hex>`, 256-bit entropy,
  prefixed so secret scanners can recognize it) is returned exactly once at
  mint time. Scopes are two arrays checked on every call: `allowed_repos`
  and `allowed_tools` (`'*'` = all; empty = nothing). Tokens are revocable
  (`revoked_at`) and optionally expiring (`expires_at`); one agent can hold
  many tokens with different scopes.
- **`agent_audit_log`** — one row per MCP tool invocation: agent, token,
  tool, repo, SHA-256 digest of the arguments, outcome
  (`ok`/`denied`/`error`), error text, timestamp. Denied calls are audited
  too. The args digest gives provenance ("this exact call produced that
  commit") without retaining potentially large or sensitive payloads. A
  failed audit write fails the tool call — the gateway does not silently
  drop provenance.

Provisioning is a small admin REST surface on the gateway (create agent,
mint token, read audit log) guarded by one operator bearer token from the
environment — deliberately boring for the prototype.

## Consequences

- The Stage 4 exit condition is testable end to end: mint a token scoped
  to `(repo X, four tools)`, and calls outside that scope come back denied
  and audited (`crates/clotho-agent-gateway/tests/agent.rs`).
- Scope checks are per-call DB-backed lookups; revocation and disabling
  take effect immediately, with no cached sessions to chase.
- Rate limits, per-run/session provenance (which prompt produced this
  call), and richer scope grammars (per-tool argument constraints) are
  vision-spec features that extend these tables without reshaping them.
- When human OAuth arrives (post-prototype), it lands in separate tables;
  the audit log's `agent_id` foreign key keeps machine provenance
  machine-only.
