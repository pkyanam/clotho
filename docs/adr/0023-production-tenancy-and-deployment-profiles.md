# ADR-0023: Production tenancy and hosted/self-host deployment profiles

- **Status:** Accepted
- **Date:** 2026-07-11
- **Deciders:** Clotho core
- **Related:** ADR-0005, ADR-0014–0016, ADR-0018–0019, ADR-0022

## Context

Clotho must be both a credible open-source self-hosted forge and a major hosted
platform. The current control plane has users, organizations, permissions,
Clerk/bootstrap authentication, scoped agents, Postgres, durable job leases,
and provider boundaries, but those pieces do not yet constitute production
multi-tenancy or an elastically operated hosted service.

Treating tenancy as an authentication feature would leave storage keys, caches,
queues, logs, webhooks, indirect IDs, provider credentials, and background work
as cross-tenant escape paths. Treating autoscaling as “run more containers”
would endanger stateful VCS, merge, database, and storage ownership.

## Decision

### 1. Organization is the tenant boundary

A Clotho organization is the security, policy, quota, billing/metering, audit,
storage, provider, and workload boundary. Every durable resource carries an
immutable Clotho tenant/org identifier. Public IDs never replace tenant context.

All public request handling resolves a typed principal and tenant context before
resource lookup. Internal APIs accept tenant-scoped identifiers or an explicit
tenant context; ambient/default organization access is forbidden in production
paths. PostgreSQL row-level security is added as defense in depth. It does not
replace application-layer authorization or tenant-scoped storage/queue/cache
namespaces.

### 2. Authentication is pluggable; authorization is Clotho-owned

Deployment profiles use the same `AuthProvider` contract:

| Profile              | Human authentication                             | Default posture                    |
| -------------------- | ------------------------------------------------ | ---------------------------------- |
| local/CI             | generated bootstrap identity and Clotho tokens   | convenient, visibly non-production |
| hosted               | Clerk initially                                  | authentication required            |
| production self-host | generic OIDC, with documented bootstrap recovery | authentication required            |

Clotho memberships, roles, repository grants, policies, and audit records remain
authoritative after authentication. Agents remain Clotho-native identities and
never become Clerk/OIDC human users. Forgejo identities remain internal provider
mappings and never become the product authorization source.

### 3. Separate control-plane scaling from workload scaling

API, MCP, and web processes may be stateless replicas. Accepted asynchronous
work is persisted before acknowledgement and executed through durable leases,
idempotent reconciliation, tenant-aware admission control, and bounded retries.

Disposable workload cells may autoscale by queue depth, latency, capability,
region, and provider capacity. Stateful Postgres, Git/VCS, merge coordination,
Arachne/object storage, and lifecycle ownership require explicit HA, sharding,
backup, restore, and failover designs; adding replicas alone is not considered
autoscaling or production readiness.

### 4. One product, supported deployment profiles

Hosted and self-hosted Clotho share source, schemas, REST behavior, migrations,
and conformance tests:

- Compose supports evaluation and small installations.
- Helm/Kubernetes is the supported production profile.
- External Postgres and object storage are supported without code forks.
- Provider credentials are connected in product or generated safely; no profile
  makes a hand-written `.env` the mandatory onboarding path.

Tenant quotas, fairness, metering, SLOs, observability, rolling upgrades,
point-in-time recovery, key rotation, incident procedures, signed artifacts,
SBOMs, and destructive restore drills are Stage 24 production gates.

## Consequences

- Stage 23 is identity/authorization/tenant-isolation work; Stage 24 is hosted
  operations and elastic workload execution. Frontier features move later.
- Existing tables and APIs must be audited and migrated where tenant identity is
  inferred through a repository name, user default, provider mapping, or global
  cache key.
- Single-node self-host remains simple, but simplicity cannot rely on bypassing
  tenant context or using globally shared secrets/namespaces.
- Billing UI is not required before metering, quotas, and cost attribution;
  hosted operations need trustworthy usage data before commercial policy.
- Production claims require adversarial isolation and failure/recovery evidence,
  not only functional multi-user tests.
