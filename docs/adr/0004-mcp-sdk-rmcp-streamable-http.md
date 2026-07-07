# ADR-0004: Build the agent gateway on rmcp with streamable HTTP transport

- **Status:** Accepted
- **Date:** 2026-07-07
- **Deciders:** Clotho core

## Context

Stage 4 (docs/prd.md §5) turns `clotho-agent-gateway` into a real MCP
server: `checkpoint`, `restore_to`, `diff_symbol`, `orient_repo` as tool
calls, guarded by scoped agent identities (ADR-0005). Two decisions
needed making: which MCP implementation, and which transport.

## Decision

**SDK: `rmcp`, the official MCP Rust SDK** (modelcontextprotocol/rust-sdk),
pinned exactly to `=2.0.0`.

- It is the reference Rust implementation, Apache-2.0, actively maintained
  by the MCP org itself — the same open/no-lock-in reasoning as jj-lib and
  xet-core (ADR-0001/0002).
- Third-party "agentic-jujutsu"-style crates stay out of the core, per
  docs/prd.md §7: unverified production-readiness claims, no org
  visibility. `rmcp` gives us transports and protocol plumbing only; every
  tool is backed by our own services over gRPC.
- Pinned exactly despite being post-1.0: upstream shipped three minor
  releases in the month before this ADR (1.8.0 → 2.0.0 → 2.1.0, with 2.0.0
  a breaking release), which is jj-lib-grade churn. Upgrades are deliberate,
  never automatic. 2.0.0 over 2.1.0: the newest release was days old at
  decision time; house policy prefers dependencies that have been public
  for at least a week.

**Transport: streamable HTTP** (`/mcp` on port 8090), not stdio.

- The gateway is a long-running containerized service that many agents
  reach over the network; stdio transport assumes the client spawns the
  server as a child process, which contradicts the whole identity/audit
  design (one shared service enforcing scopes centrally).
- Streamable HTTP is the current MCP spec's remote transport (it replaced
  SSE-only in spec 2025-03-26), and it is plain HTTP: bearer tokens ride
  the `Authorization` header, so agent auth is ordinary web middleware.
  Auth resolves a token to an agent identity *before* the MCP layer sees
  the request; the transport forwards the request parts (with the resolved
  identity in the extensions) into every tool handler.

## Consequences

- Any spec-compliant MCP client (Devin CLI, Claude Code, an rmcp client)
  can use the gateway with `url + Authorization: Bearer <token>` — no
  Clotho-specific client code. Verified end to end by
  `crates/clotho-agent-gateway/tests/agent.rs`.
- The MCP server is stateful per session (`Mcp-Session-Id`, in-memory
  `LocalSessionManager`): sessions do not survive a gateway restart, and a
  multi-replica deployment would need sticky routing or a shared session
  store. Fine for the prototype; revisit before any horizontal scaling.
- rmcp 2.x tracks the MCP spec closely; expect breaking changes at minor
  bumps and budget for them like jj-lib upgrades.
