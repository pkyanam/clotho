# ADR-0018: AuthProvider — Clerk for humans, Clotho for agents

- **Status:** Accepted
- **Date:** 2026-07-09
- **Deciders:** Clotho core
- **Supersedes (partially):** ADR-0015 bootstrap-as-default human auth path
  (bootstrap remains the local/dev AuthProvider; production humans move to Clerk)

## Context

ADR-0015 shipped human API tokens (`api_tokens`) and optional
`CLOTHO_AUTH_REQUIRED`, with a bootstrap user fallback for open local/dev.
That is enough for a prototype control plane, not for a world-class forge:

- No SSO, social sign-in, MFA, or polished org invitation UX.
- Human identity is still thin compared to agent identity (ADR-0005), which
  already has scoped tokens and audit.
- Vision §1 and the Dream Roadmap require Clotho to be open/self-hostable
  *and* delightful for hosted users — locking human auth to a single SaaS
  forever would violate the modular stance; inventing full IdP UX would
  burn novelty budget that belongs on Arachne, merge-queue, and agents.

Clerk provides B2B organizations, session UI, and org-scoped API keys.
Agents must **not** become Clerk users: ADR-0005's non-human identity
primitive is a product differentiator and must stay Clotho-owned.

## Decision

### 1. Introduce an `AuthProvider` boundary

Human authentication is pluggable behind a Clotho-owned interface (conceptually
parallel to CCI for compute):

```text
AuthProvider
  verify_session(token) -> HumanPrincipal   # web session / JWT
  verify_api_key(token)  -> HumanPrincipal   # CLI / SDK / scripts
  resolve_org(external_org_id) -> ClothoOrgId
  list_memberships(principal) -> [OrgMembership]
```

Implementations:

| Provider id | Role | When |
|---|---|---|
| `bootstrap` | Dev/demo: existing ADR-0015 tokens + bootstrap user | Default when Clerk is unset |
| `clerk` | Hosted human SSO, orgs, Clerk org API keys | Default for managed Clotho |
| `oidc` (later) | Generic OIDC/SAML for self-host purity | After Clerk path is stable |

Clotho services depend only on `AuthProvider` + Clotho Postgres mappings —
never on Clerk SDK types leaking into VCS, storage, or MCP crates.

### 2. Clerk owns humans; Clotho owns agents

| Identity | System of record | Credentials | Surfaces |
|---|---|---|---|
| Humans / orgs | **Clerk** (when `clerk` provider active) | Session JWT, Clerk org API keys | Web, human CLI/SDK |
| Agents | **Clotho** (`agents`, `agent_tokens`, `agent_audit_log`) | `clotho_agt_…` bearer | MCP, agent admin via edge (ADR-0016) |

Hard rules:

- Never create a Clerk user for an agent.
- Never put agent scopes on a Clerk API key.
- Agent admin remains human-only (ADR-0016).
- Map `clerk_user_id` → `clotho.users` and `clerk_org_id` → `clotho.orgs`
  (or equivalent link tables). Clotho permissions (`repo_permissions`, org
  roles) remain the authorization source for product actions after the
  principal is resolved.

### 3. Auth required by default outside local/dev

- Managed / production profiles: `CLOTHO_AUTH_REQUIRED=true` and
  `CLOTHO_AUTH_PROVIDER=clerk`.
- Local compose / CI: `bootstrap` provider may keep open auth for tests
  unless a test explicitly enables required auth.
- ADR-0015 token table may remain as a Clotho-minted human API key path
  *or* be superseded by Clerk org API keys for the `clerk` provider —
  product choice at implementation time, but both must resolve to the same
  `HumanPrincipal` + Clotho permission checks.

### 4. Web integration shape

- Next.js app uses Clerk's App Router helpers for sign-in/sign-up/org switcher.
- Server-side `api()` calls the gateway with a verified session-derived Bearer
  (or gateway-trusted internal header after edge verification) — never
  expose Clerk secret keys to the browser beyond publishable config.
- Gateway validates Clerk session/JWT (or Clerk Backend API) when provider
  is `clerk`; validates `clotho_tok_…` when provider is `bootstrap`.

## Consequences

- Human onboarding becomes product-grade without rewriting agent identity.
- Self-host story stays honest: swap `AuthProvider` to `oidc` / `bootstrap`
  without changing REST/MCP contracts.
- New dependency: Clerk for managed deployments; document
  `CLERK_PUBLISHABLE_KEY` / `CLERK_SECRET_KEY` (and org settings) in
  `.env.example` as managed-only, not required for `just demo`.
- Migration work: sync or link existing bootstrap orgs/users to Clerk orgs
  on first connect; keep Forgejo identity as internal mapping only
  (PRD v2 assumption unchanged).
- Open decision: whether Clotho continues minting its own human API tokens
  alongside Clerk keys, or fully delegates human machine auth to Clerk.

## Implementation note (2026-07-09, Stage 17)

**§11 #7 default:** keep minting Clotho `clotho_tok_…` under both `bootstrap`
and `clerk`. Under `clerk`, Bearer resolution order is: Clotho token → Clerk
session JWT → Clerk Backend API key probe. Both resolve to the same
`AuthContext` and Clotho permission checks. Agents remain `clotho_agt_…` only.
