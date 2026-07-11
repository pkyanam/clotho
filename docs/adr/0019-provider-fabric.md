# ADR-0019: Provider Fabric — pluggable compute, storage, and network

- **Status:** Accepted
- **Date:** 2026-07-09
- **Deciders:** Clotho core
- **Related:** ADR-0008 / ADR-0013 (CCI), ADR-0014 (secrets), ADR-0018 (AuthProvider),
  ADR-0020 (Arachne↔VCS), ADR-0021 (Tailscale), ADR-0022 (agent runtime)

## Context

Vision §4 and the Dream Roadmap require Clotho to feel "as simple as Vercel,
as robust as Cloudflare": opinionated defaults with real primitives underneath
that operators can swap. Today only **compute** has a mature pluggable
boundary (CCI + ProviderRegistry). Storage is MinIO/env-wired inside
`clotho-storage`; networking is public egress only; auth is bootstrap tokens.

Customers who want "best forge" also want:

- Bring-your-own S3/R2/GCS for Arachne chunks (data residency, cost, lock-in).
- Bring-your-own compute (already partially there) **and** private network
  reach into their devices/VPC (Tailscale — ADR-0021).
- Capability-oriented UX: declare `needs: [gpu, private-net, persistent]`,
  not vendor names in day-to-day workflows.

Without a shared fabric pattern, each new BYO surface will invent its own
connect/disconnect/secrets/UI dialect and drift from REST/CLI/MCP parity.

## Decision

### 1. Provider Fabric as the product pattern

Every major infra subsystem follows the same shape:

```text
*Provider (trait / registry)
  id, kind, configured?, capabilities, non-secret metadata
  connect(credentials) / disconnect()
  resolve(job | repo | org context) -> concrete backend
```

| Fabric layer | Boundary name | Default | BYO examples |
|---|---|---|---|
| Auth | `AuthProvider` (ADR-0018) | `bootstrap` | Clerk, later OIDC |
| Compute | `ComputeProvider` / CCI (ADR-0013) | Daytona / disabled | Box, ComputeSDK upstreams, `clotho-runner` |
| Storage | `ObjectStoreProvider` | Clotho MinIO | Customer S3, R2, GCS, Azure Blob |
| Network | `NetworkProvider` | `public` | Tailscale (ADR-0021), later generic WireGuard |

Collab (Forgejo) stays an **internal** provider behind the facade (ADR-0011) —
not a customer-facing BYO slot unless we later offer "external forge" mode.

### 2. ObjectStoreProvider (BYO S3)

`clotho-storage` already implements Arachne over S3-compatible stores via
`object_store` (ADR-0002). Elevate that to a product surface:

- Org (and optionally repo) may **connect** a storage backend:
  endpoint, region, bucket, path prefix; credentials sealed via ADR-0014
  secrets (`POST /api/v1/providers/storage/connect` or org-scoped equivalent).
- `configured` means "can read/write a probe object with current credentials"
  — same honesty rule as compute (Stage 14).
- Unconnected orgs use the Clotho-managed default bucket (dev: MinIO).
- Arachne CAS layout (`shards/`, xorbs) is provider-agnostic; only the
  object-store endpoint changes.

Repo kinds and VCS wiring are specified in ADR-0020; this ADR only owns the
pluggable object-store slot.

### 3. NetworkProvider slot

Introduce a network provider registry with at least:

- `public` — today's behavior (no mesh).
- `tailscale` — org OAuth client + tagged ephemeral/BYOC nodes (ADR-0021).

Compute jobs may declare `requires_private_net: true` (or capability
`private-net`). The fabric refuses to schedule on providers that cannot
satisfy the network requirement when a NetworkProvider is connected.

### 4. Capability resolution, not vendor pickers

Actions, sandboxes, and agent sessions specify **capabilities**. The fabric:

1. Filters providers that advertise those capabilities and are `configured`.
2. Applies org/repo defaults and optional explicit `provider_id` override.
3. Injects secrets (ADR-0014) and network join credentials as needed.

UI settings pages list providers by layer (Compute / Storage / Network /
Auth) with connect/disconnect — never raw secrets.

### 5. REST / SDK / CLI / MCP parity

New routes follow Stage 15 contract rules:

- `GET /api/v1/providers?layer=compute|storage|network|auth`
- Existing compute connect/disconnect remain; storage/network gain analogous
  connect/disconnect + metadata-only responses.
- OpenAPI + SDK + CLI groups updated in the same change set as the routes.
- MCP may `list_providers` across layers (metadata only); never mint storage
  or Tailscale credentials.

## Consequences

- CCI remains the compute implementation of the fabric, not a special case.
- Secrets store becomes the universal credential vault for all BYO layers.
- Implementation can land incrementally: storage connect first (Phase B),
  Tailscale second (Phase C), without rewriting Actions.
- Risk: over-abstracting before two real implementations exist — mitigate by
  shipping ObjectStoreProvider with MinIO + one customer S3 path before
  inventing a generic plugin SDK for third parties.
- Out of scope here: customer-attached *query* databases (vision §4.3 #2) —
  that remains a later connector framework, not ObjectStoreProvider.
