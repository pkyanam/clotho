# ADR-0009: Agent write tools route through VCS and merge-queue over MCP

- **Status:** Accepted
- **Date:** 2026-07-08
- **Deciders:** Clotho core

## Context

Stage 7 closed the prototype, but recorded one concrete ergonomics gap:
agents could orient, checkpoint, restore, and inspect structured diffs over
MCP, but had no write path. The demo therefore committed over raw
`clotho-vcs` gRPC before submitting to the merge queue. That bypassed the
agent identity/audit model from ADR-0005 and made Stage 6 presence blind to
agent-authored writes (ADR-0007's known limitation).

The vision spec (§3.4) says agents should use a native structured interface,
not service internals.

## Decision

Add two MCP tools to `clotho-agent-gateway`:

- **`commit`** calls `clotho-vcs.Commit` with explicit file contents,
  deletions, parent ids, message, and author metadata. When author metadata is
  omitted, the gateway derives it from the authenticated agent identity.
- **`submit_change`** calls `clotho-merge-queue.SubmitChange`, which serializes
  integration and delegates landing/rebase/conflict handling to `clotho-vcs`
  per ADR-0006.

Both tools use the existing scoped-token model unchanged:
`allowed_tools` now names `commit` and `submit_change` like any other tool,
`allowed_repos` gates the target repo, and every allowed, denied, and failed
call lands in `agent_audit_log` before the MCP response returns. This means
write activity appears in the same presence panel as read/checkpoint activity.

The first `commit` payload is text-first (`content` as UTF-8). Binary/artifact
writes should become a later storage-aware tool instead of overloading the MCP
JSON path.

## Consequences

- Agents can author and submit a change entirely over MCP under their scoped
  identity; no raw VCS or merge-queue gRPC client is required for normal agent
  writes.
- The write tools intentionally keep the gateway thin. VCS still owns object
  creation; merge-queue still owns serialized landing; the agent gateway owns
  identity, authorization, and audit.
- `open_pr` and `request_review` remain deferred. They cross the Forgejo API
  boundary, and Stage 8 keeps Forgejo API-level only. A later ADR should decide
  their exact api-gateway shape before exposing them over MCP.
