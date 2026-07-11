# ADR-0016: Agent admin via the REST edge

## Status

Accepted

## Context

Agent identity (agents, scoped tokens, audit log) lives in the agent-gateway
Postgres schema ([ADR-0005](0005-agent-identity-schema.md)). Stage 6 added
repo presence by proxying the gateway admin sessions endpoint through the
api-gateway ([ADR-0007](0007-edge-read-api-and-presence.md)).

Humans need to create agents, mint scoped tokens, revoke credentials, and
read audit history without calling the agent-gateway admin surface directly.
The product contract is the public REST API ([`openapi.yaml`](../openapi.yaml));
the web app and CLI must not talk to agent-gateway as a separate product.

Slice A added human API tokens and permission checks on the edge
([ADR-0015](0015-human-api-tokens.md)).

## Decision

1. **Canonical admin API** — `GET/POST /api/v1/agents`, token lifecycle under
   `/api/v1/agents/{name}/tokens`, and `GET /api/v1/agents/{name}/audit` on the
   api-gateway. The edge proxies to agent-gateway `/admin/v1/*` using
   `CLOTHO_AGENT_ADMIN_TOKEN` (service-to-service).

2. **Human auth** — Every agent admin route calls `auth::resolve_auth`. Caller
   must be the bootstrap user or hold **org admin** membership in any org when
   Postgres is configured. Repo sessions (`/api/v1/repos/{name}/agent-sessions`)
   require authentication but not org admin (read-only presence).

3. **Gateway configuration** — If `CLOTHO_AGENT_ADMIN_TOKEN` is unset on the
   api-gateway, agent admin routes return **503** with
   `agent management is not configured`.

4. **MCP boundary** — Agent admin (create, mint, revoke, scope edits) is
   **human-only** via web, CLI, and REST. MCP tools must not mint or revoke
   peer agent credentials. Agents receive tokens from operators out of band.

5. **Token semantics unchanged** — Plaintext agent tokens (`clotho_agt_…`) are
   returned once at mint time; list/detail endpoints expose metadata only.

## Consequences

- Web `/agents` and `clotho agent …` manage identities through the same REST
  surface as automation.
- Agent-gateway admin routes remain for service-to-service use; operators should
  prefer the edge in product flows.
- Slice D can add org-scoped agent policies or delegate admin to repo admins
  without changing the proxy shape.

## References

- [`docs/api.md`](../api.md) — agents admin section
- [`docs/mcp.md`](../mcp.md) — explicit non-goals for peer credential tools
