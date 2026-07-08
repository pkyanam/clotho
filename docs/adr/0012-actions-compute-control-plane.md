# ADR-0012: Clotho owns Actions and the compute control plane

- **Status:** Accepted
- **Date:** 2026-07-08
- **Deciders:** Clotho core

## Context

Stage 7 wired CI end to end: Forgejo push webhook, api-gateway orchestration,
`clotho-compute` through the CCI, Daytona sandbox execution, and a Forgejo
commit status. Stage 9 made Clotho's web/API the primary collaboration
surface. The remaining gap is that compute still appears to users only as a
commit status, while the vision spec calls for configurable, provider-agnostic
compute/runners/sandboxes.

## Decision

Clotho owns **Actions** as a first-class product surface and API. Action runs
are Clotho records with status, jobs, logs, provider metadata, sandbox ids, and
timestamps. Forgejo commit statuses are an output/sync target for compatibility
with PR review, not the source of truth for the Actions UX.

`clotho-compute` remains provider-agnostic behind the CCI trait from ADR-0008.
Daytona is the first configured provider. The API gateway keeps orchestration
for Stage 10 because CI webhook handling and Forgejo status sync already live
there; compute continues to run commands only and remains collaboration-agnostic.

The browser and SDK talk only to `clotho-api-gateway`:

- `/repos/{repo}/actions/...` for runs, logs, and repo config;
- `/compute/providers...` for configured provider metadata.

Secrets are never returned to the browser. The first provider surface reports
whether Daytona is configured from environment and exposes non-secret defaults
such as snapshot/image and timeout.

Forgejo remains an internal unmodified provider. This ADR does not resolve PRD
§8 #1 or #2.

## Consequences

- Users can inspect CI/runner state without opening Forgejo.
- PRs still get commit statuses, preserving compatibility with the Stage 9
  collaboration facade.
- The first Stage 10 run store is intentionally lightweight. Durable Actions
  history should move to Postgres, or a separate `clotho-actions` service, once
  retention, pagination, org policy, and permissions are designed.
- Adding another compute backend remains a new CCI provider implementation plus
  provider metadata/config mapping, not a frontend rewrite.
