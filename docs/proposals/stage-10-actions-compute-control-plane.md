# Stage 10 Proposal: Actions + Compute Control Plane

**Status:** Proposed
**Date:** 2026-07-08
**Context:** Post-Stage 9 product shell and collaboration facade

## Summary

Stage 10 makes compute visible and controllable from Clotho itself. The Stage 7
prototype already proved push webhook -> CCI -> Daytona sandbox -> commit
status. Stage 10 turns that into a first-class product surface:

- Actions are Clotho-owned records and pages, not only Forgejo commit statuses.
- Daytona remains the first real compute provider behind the CCI from ADR-0008.
- Commit statuses remain an output for PR compatibility.
- Users configure runner/sandbox behavior through the Clotho API and web app.
- Forgejo remains internal and unmodified.

## Goals

1. A repo has an **Actions** section that lists runs, status, provider, sandbox,
   commit, trigger, duration, and logs.
2. Push-triggered CI creates and updates an action run before syncing the final
   result to Forgejo commit statuses.
3. Users can manually start a run for a commit through the Clotho API/UI.
4. The web app shows compute provider state, including whether Daytona is
   configured, without exposing secrets.
5. The API exposes a small runner/sandbox configuration surface that can grow
   into persistent org/repo settings.

## Non-goals

- Do not build Cloudflare deployment, Workers, Durable Objects, or edge runner
  infrastructure in this stage.
- Do not add paid services or require new keys beyond the existing optional
  Daytona configuration.
- Do not expose raw `DAYTONA_API_KEY` or any secret value to the browser.
- Do not modify Forgejo source or resolve PRD §8 license/fork decisions.
- Do not replace the CCI trait or hardcode Actions to Daytona.

## Proposed API

```text
GET  /api/v1/repos/{repo}/actions/runs
POST /api/v1/repos/{repo}/actions/runs
GET  /api/v1/repos/{repo}/actions/runs/{run_id}
GET  /api/v1/repos/{repo}/actions/runs/{run_id}/logs

GET  /api/v1/repos/{repo}/actions/config
PUT  /api/v1/repos/{repo}/actions/config

GET  /api/v1/compute/providers
GET  /api/v1/compute/providers/{provider}
```

Initial records:

- `ActionRun`: id, repo, commit id, branch, status, conclusion, trigger, actor,
  provider, sandbox id, timestamps, duration, and job list.
- `ActionJob`: id, run id, name, status, exit code.
- `ActionLog`: run id plus log text.
- `ComputeProvider`: id, display name, enabled/configured flags, capabilities.
- `ActionsConfig`: enabled flag, provider, default image/snapshot, timeout.

## Implementation Direction

Keep CI orchestration in `clotho-api-gateway` for the first pass because the
webhook and Forgejo status sync already live there. `clotho-compute` remains
the provider-agnostic execution service and continues to know nothing about
Forgejo, PRs, or Actions.

The first API-backed implementation may use gateway-local state to establish
the contract. Before Actions are treated as durable CI history, move run
metadata and logs into Postgres or split a small `clotho-actions` service if
the orchestration surface grows.

## Exit Condition

Stage 10 is complete when a user can open a repo, view Actions, see a
Daytona-backed run with logs/status, inspect provider configuration, manually
run the default check, and see PR commit statuses still reflect the run result
without opening Forgejo.
