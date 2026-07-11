# ADR-0014: Secrets storage and resolution path

- **Status:** Accepted
- **Date:** 2026-07-08
- **Deciders:** Clotho core

## Context

Stage 12 compute providers (Daytona, ComputeSDK bridge, Box stub) are env-backed
(`DAYTONA_API_KEY`, `BOX_API_KEY`, bridge URL). That is a fine bootstrap for
local compose, but it is not a product UX: operators will remove provider keys
from `.env`, and the web app must not tell users to edit host environment files.

PRD Stage 13 and the vision (“as simple as Vercel / as robust as Cloudflare”)
require first-class secrets at org and repo scope, with provider credentials
bound to named secrets. Encrypted vault services (HashiCorp Vault, cloud KMS)
are correct long-term; Stage 13 needs something small, reviewable, and
self-hostable that does not block the product surface.

## Decision

### 1. Store secrets in control-plane Postgres

New table `secrets` (migration `1003_secrets.sql`):

| Column | Purpose |
|---|---|
| `id` | UUID primary key |
| `scope` | `org` \| `repo` (platform scope deferred) |
| `org_id` / `repo_id` | FK to control-plane rows |
| `name` | Unique within scope (e.g. `DAYTONA_API_KEY`) |
| `description` | Optional non-secret text |
| `ciphertext` | AES-256-GCM sealed blob (nonce ‖ ciphertext ‖ tag) |
| `value_last4` | Optional mask for UI (`···x7k2`) |
| `created_by`, `created_at`, `updated_at` | Audit metadata |

**Choice: encrypted column in Postgres**, not an external vault for Stage 13.

Rationale:

- Already have control-plane Postgres and migrations (Stage 11).
- One deploy unit; works offline / single-node self-host.
- Honest about limits: protection is app-level encryption with a server master
  key, not hardware isolation or per-tenant HSM.

### 2. Master key from environment

`CLOTHO_SECRETS_MASTER_KEY` is a **bootstrap** secret (like DB URLs). It is the
only provider-related credential that belongs in host env / compose secrets.

- 32-byte key, base64 or 64-char hex.
- Used only by `clotho-api-gateway` to seal/unseal secret values.
- Rotation: re-encrypt all rows with a new key (tooling later); for Stage 13
  document “rotate by setting a new key and re-creating secrets.”
- If unset: list/metadata still works; **write** and **resolve** return a clear
  configuration error so the stack stays up without silent plaintext.

### 3. API contract (never return raw values)

```
GET/POST   /api/v1/orgs/{org}/secrets
GET/PATCH/DELETE /api/v1/orgs/{org}/secrets/{name}
GET/POST   /api/v1/repos/{repo}/secrets
GET/PATCH/DELETE /api/v1/repos/{repo}/secrets/{name}
POST       /api/v1/providers/{id}/connect   # convenience: store key as org secret
```

Responses: `{ name, scope, description, value_last4, updated_at, … }` — **never**
the plaintext after write. Create accepts `value` once; rotate = PUT/PATCH with
a new value.

### 4. Resolution path for compute

1. **Env escape hatch (dev):** if `DAYTONA_API_KEY` (etc.) is set on
   `clotho-compute`, that provider is configured as today.
2. **Clotho secret (primary):** well-known names at org scope (fallback: platform
   later):
   - `DAYTONA_API_KEY` → Daytona
   - `BOX_API_KEY` → Box
   - optional binding metadata on provider connect
3. **At Actions run time:** api-gateway resolves the secret for the repo’s org
   (repo-scoped override if present), decrypts in-process, and passes it on the
   CCI `RunJob` request as `provider_credentials` (proto field 8). The value
   never enters the browser, logs, or activity payloads.
4. **ListProviders / settings UI:** gateway overlays compute’s env-based
   `configured` flag with “secret present in Clotho” so the console shows
   **Configured · ···x7k2** without process restart.

`clotho-compute` Daytona (and future providers) prefer per-job
`provider_credentials.api_key` over process env, so empty `.env` works when
Clotho secrets are set.

### 5. Audit

`activity_events` records `secret.created` / `secret.updated` / `secret.deleted`
with **name and scope only** (no ciphertext, no last4 of a delete of the value).

### 6. Permissions

Org-secret writes and metadata reads require an administrator of that
organization. Repo-secret writes and metadata reads require a repository
administrator or an administrator of the owning organization. Authentication
and authorization run before a metadata name lookup, so a caller without the
required role cannot distinguish an existing secret name from an absent one.
An explicitly supplied invalid credential fails even when local bootstrap
fallback is enabled. Deny by default when pool/auth is missing (routes require
DB).

## Consequences

- Operators can delete `DAYTONA_API_KEY` from `.env` and configure keys in the
  web console.
- `.env.example` documents bootstrap secrets only (`CLOTHO_SECRETS_MASTER_KEY`,
  DB URLs); provider keys are secondary “dev escape hatch” comments.
- Not a substitute for Vault/KMS at enterprise scale; a later stage can swap the
  seal backend behind the same REST contract.
- Proto change: `RunJobRequest.provider_credentials` (field 8) — internal CCI
  only; not a public REST field.

## Alternatives considered

| Option | Why not for Stage 13 |
|---|---|
| HashiCorp Vault / cloud KMS | Operational weight; not required for product UX |
| Encrypt only in browser / localStorage | Secrets would leave the server trust boundary |
| Store plaintext in Postgres | Unacceptable even for prototype maturity |
| Keep env-only | Product owner deletes keys; compute dies |

## References

- docs/prd.md §5 Stage 13
- docs/adr/0012-actions-compute-control-plane.md
- docs/adr/0013-compute-provider-registry.md
