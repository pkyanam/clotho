# Stage 22 scoped MCP-to-REST authorization

**Date:** July 11, 2026

REST-backed MCP tools now preserve the originating scoped agent as the
authority. The agent gateway retains the opaque bearer in request-local state
and forwards it on each REST request. The API gateway recognizes only the
agent-token prefix, calls the agent gateway's internal admin-authenticated
`POST /admin/v1/authorize`, and supplies the repository and tool chosen by the
REST handler. The response contains immutable agent/token ids, display name,
and allowed repositories; it never contains the bearer or allowed-tool set.

The common gates cover issue and pull list/mutation tools, repository tree and
file reads, Action list/start/log tools, repository and activity pages,
provider capability listing, and repository secret metadata. Tool/repository
scope is revalidated before provider, VCS, compute, or pagination work.
Unauthorized repository reads use the same safe `404` as a missing repository;
invalid, expired, revoked, or disabled tokens return `401`; mutation/tool
denials return `403`. Agent credentials never fall back to a human or the
open-local bootstrap identity.

Action request bodies cannot forge attribution: persisted actor is
`agent:<authenticated-name>`. Persisted idempotency scope is
`agent:<agent-id>:token:<token-id>`, preventing keys from aliasing across
credentials. Repository/activity results are filtered against allowed
repositories before page limits, duplicate global names are omitted, provider
details are sanitized, organization secret listing is denied, and repository
secret results remain metadata-only.

Verification for this slice includes gateway and agent-gateway unit/integration
tests, the live `just test-agent` MCP client, direct REST-equivalence assertions
using that same agent bearer, required-auth Docker probes, contract checks, and
a gateway restart without removing volumes. Exact final commands and results
are recorded in `docs/handoff.md`; credential-gated provider checks are not
represented as exercised by this authorization evidence.

Remaining Stage 22 work is explicit: organization/user directory membership
filtering and all human provider-metadata views still need their own
two-organization adversarial slice. This evidence is not a Stage 23 tenant
isolation claim.
