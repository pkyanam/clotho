# ADR-0015: Human API tokens and permission enforcement

**Status:** Accepted  
**Date:** 2026-07-09  
**Context:** Slice A of the web control plane maturity work.

## Decision

Clotho's api-gateway owns **human API tokens** stored in Postgres (`api_tokens`).
Clients authenticate with `Authorization: Bearer clotho_tok_…`.

### Bootstrap

On first boot, `ensure_bootstrap` mints a token for the bootstrap user when none
exists. The plaintext is logged once at INFO. For deterministic demos, set
`CLOTHO_BOOTSTRAP_TOKEN` before start (hashed and upserted).

### Auth resolution

Every request resolves an `AuthContext` (`user_id`, optional `token_id`):

1. **Bearer present** — validate token hash; 401 if invalid/expired/revoked.
2. **No Bearer, `CLOTHO_AUTH_REQUIRED=false`** (default) — fall back to the
   bootstrap user (preserves open local/dev/tests).
3. **No Bearer, `CLOTHO_AUTH_REQUIRED=true`** — 401 Unauthorized.

When a pool exists, **mutating routes** check org/repo permissions for the
resolved actor. Missing permission → 403 Forbidden.

### Permission model

| Scope | Roles / permissions | Ordering |
|---|---|---|
| Org | `admin`, `member` | admin > member |
| Repo | `admin`, `write`, `read` | admin > write > read |

Org admins inherit access to repos in that org. Repo-level grants are stored in
`repo_permissions`.

Typical gates:

- Create org — authenticated (creator becomes org admin)
- Create repo — org admin of target org
- Commit, submit, issues, PRs — repo `write`
- Repo settings PATCH, secrets — repo `admin` or org admin
- Delete repo — repo `admin` or org admin
- Org secrets, provider connect — org `admin`

### Public API hygiene

Responses never expose internal provider field names (`forgejo`,
`forgejo_owner`). Repo detail uses top-level `description` and nested `info`.
Clone URLs use `CLOTHO_PUBLIC_GIT_URL` (default `http://localhost:13000`).

### Client wiring

- **SDK / CLI / web server:** `CLOTHO_TOKEN` (or `--token` on CLI)
- **Web:** `CLOTHO_TOKEN` / `CLOTHO_API_TOKEN` on server-side `api()` for v1

Agent bearer tokens remain on the agent-gateway (Slice C).

## Consequences

- Existing integration tests keep working with default open auth.
- Production can enable `CLOTHO_AUTH_REQUIRED=true` without code changes.
- Token plaintext is shown only at mint time; storage is SHA-256 hex only.
