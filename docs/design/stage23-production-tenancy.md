# Stage 23 design — production identity and tenant isolation

**Status:** approved direction; implementation begins only after Stage 22 is
materially closed.

**Governing decisions:** ADR-0018, ADR-0023. Signals are later Stage 26 work
governed by ADR-0024.

## Goal

Make a Clotho organization an enforceable tenant boundary for multiple humans,
agents, repositories, providers, secrets, data, and workloads in both hosted and
self-hosted production profiles.

This stage is not “add login screens.” It closes every place where identity or
tenant context can be lost between authentication, authorization, persistence,
asynchronous work, providers, and responses.

## Invariants

- Authentication adapters resolve humans; Clotho authorizes product actions.
- Clerk is hosted convenience, generic OIDC is the production self-host path,
  and generated bootstrap identity remains local/CI only.
- Agents remain Clotho-native non-human principals.
- Tenant context is explicit and immutable; never infer it from a repo name,
  default user, Forgejo owner, storage prefix, or untrusted request content.
- A denied or foreign indirect ID does not reveal tenant metadata.
- Secrets stay write-only. Tenant identifiers never become secret material.
- Forgejo remains an unmodified internal provider and cannot grant Clotho
  authority.
- REST leads; SDK, CLI, MCP, and web adopt the same membership and permission
  semantics.

## Required model

`PrincipalContext` should distinguish human, agent, and service actors and carry
the authenticated subject, active tenant, memberships/role, credential identity,
delegation/impersonation facts, and request/audit correlation. A separate typed
`TenantContext` should be required by storage, queue, cache, provider, and
background-operation entry points even when no human principal is present.

Every durable resource must be classified as:

1. tenant-owned;
2. globally public and immutable;
3. operator-owned internal state.

“Global because it lacks an org column” is not a valid fourth category.

## Implementation sequence

### Slice A — inventory and deny-by-default harness

- Produce a machine-readable inventory of stable routes/tools, durable tables,
  queues/jobs, cache keys, object prefixes, webhooks, providers, and internal
  RPCs with their tenant source and authorization rule.
- Add the route × principal × tenant expectation matrix to tests.
- Introduce typed principal/tenant context at the REST boundary without changing
  successful behavior.
- Add adversarial two-org fixtures with timing-safe not-found assertions.

**Acceptance:** CI fails when a new route/resource family has no declared tenant
source or authorization rule.

### Slice B — one high-risk family end to end

Migrate secrets first unless the inventory finds a more severe escape path.
Cover org and repo secret metadata, encrypted rows, provider resolution, audit,
CLI/SDK/web, and negative indirect-ID tests. Add Postgres row-level-security
policy only after application queries carry explicit tenant context.

**Acceptance:** hostile Org B cannot infer existence, metadata, last-four,
provider configuration, timing, or audit facts for Org A; restart and key-file
behavior remain correct.

### Slice C — membership lifecycle and production IdP parity

- Add invitation, acceptance, expiry, removal, role change, org switching,
  last-owner protection, session/token revocation, and explicit delegation.
- Implement generic OIDC without leaking provider-specific types into product
  services.
- Verify Clerk, OIDC, and bootstrap principals reach the same authorization
  decisions and stable REST errors.

### Slice D — all resource and asynchronous boundaries

Migrate repositories, artifacts/releases, imports, Actions/logs, agents/audits,
providers, storage objects, queues, caches, webhooks, and background completions.
Add RLS, namespace checks, quota hooks, and audit correlation after each family
has explicit application-layer context.

## Stage acceptance

Two hostile organizations exercise all public resource families concurrently.
Neither can infer, read, mutate, schedule, starve, or retain references to the
other through direct IDs, pagination, timing, logs, storage, caches, agents,
provider credentials, webhooks, revoked memberships, or background work.

The acceptance report must include migrations, compatibility impact, a complete
deny matrix, Docker/restart evidence, Clerk/OIDC tests that were skipped for lack
of credentials, and the next Stage 24 production-control-plane slice.
