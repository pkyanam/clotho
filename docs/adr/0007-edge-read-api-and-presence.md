# ADR-0007: The web app reads only the api-gateway; presence proxies the agent gateway's audit log

- **Status:** Accepted
- **Date:** 2026-07-07
- **Deciders:** Clotho core

## Context

Stage 6 (docs/prd.md §5) builds `apps/web` into a product shell: repo
browser, PR view with the structured diff, and an agent-session presence
panel. The PRD's layering (§2) puts one boring REST/JSON edge in front of
everything — but until now the api-gateway had exactly one endpoint
(`POST /api/v1/repos`, ADR-0003), and the data the frontend needs lives in
three places: clotho-vcs (trees, files, commits, op log), Forgejo (PR
objects), and the agent gateway's Postgres (audit log → presence).

## Decision

**The frontend talks only to `clotho-api-gateway`.** It grew the read
surface under `/api/v1/repos/...`: repo list/detail, tree, file contents,
commit log, op log, PR list/detail, the structured PR diff, and agent
sessions. Three composition rules keep the layering honest:

- **clotho-vcs is the source of truth for repository state.** New vcs RPCs
  `GetFile` and `LogCommits` (proto/clotho/vcs/v1/vcs.proto) serve the
  browser; Forgejo is proxied only for what it owns — project entries and
  pull requests (still the GPLv3 API boundary of ADR-0003, no Forgejo code
  linked).
- **The PR diff is the same composition the agent gateway uses** (vcs
  `DiffCommits` → clotho-diff `DiffFiles`): one structured-diff object
  feeds both audiences (docs/prd.md §2). The edge additionally computes
  line hunks (`similar`, Myers over lines) because humans read hunks while
  agents read symbols — presentation, so it lives at the edge, not in
  clotho-diff.
- **Agent presence is proxied, not queried.** The agent gateway owns the
  identity schema (ADR-0005); the api-gateway never touches that Postgres.
  Instead the agent gateway's admin surface gained
  `GET /admin/v1/repos/{repo}/sessions` — audit-log entries aggregated per
  (agent, token) "session" — and the api-gateway proxies it with a
  service-to-service admin token (`CLOTHO_AGENT_ADMIN_TOKEN`). The web app
  polls; the PRD explicitly allows polling over real-time infra for the
  prototype.

**Conflicts stay first-class all the way to the browser** (ADR-0006's
loose end): vcs read RPCs now materialize unresolved conflict entries
(jj's marker text, the same bytes the backing git tree holds) and flag
them (`conflicted` on tree entries, file contents, and diff files) instead
of skipping them. The diff proto itself gains no conflict semantics — the
flag rides on the vcs layer, where conflicts live.

**`@clotho/sdk-js` stays hand-written** against this surface (typed,
dependency-free, unit-tested with a mocked fetch). OpenAPI generation is
deferred until the surface stabilizes — with one consumer, a spec pipeline
is ceremony, and the SDK compiling against the web app is the contract
test that matters today.

## Consequences

- A human can browse a repo, review a PR's structured diff (symbols +
  hunks, conflicts flagged and materialized), and see recent agent
  sessions — verified end-to-end by `crates/clotho-api-gateway/tests/stage6.rs`
  and the presence step of `crates/clotho-agent-gateway/tests/agent.rs`
  (both env-gated; `just test-collab` / `just test-agent`; CI runs them
  against real services).
- The read API is unauthenticated and CORS-permissive for the prototype
  (the web app is a separate origin in dev — Next on :3100, gateway
  on :8080). Human auth belongs to a later stage; the admin token never
  reaches the browser.
- Presence is derived purely from MCP tool calls through the agent
  gateway. Raw gRPC writes to clotho-vcs (as in the integration tests) are
  invisible to it — acceptable, since real agents enter through MCP.
- The gateway returns full file contents and whole-file hunks with no
  pagination; fine at prototype scale, revisit before large repos.
