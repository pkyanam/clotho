# Stage 9 Proposal: Product shell and collaboration facade

**Status:** Proposed
**Date:** 2026-07-08
**Context:** Post-prototype follow-up after Stage 8

## Summary

Stage 9 should make Clotho feel like the product described in
`docs/vision-spec.md`: a serious GitHub/GitLab replacement surface for humans
and agents, not a thin repo browser with links out to Forgejo.

The architectural direction is **not** to remove Forgejo immediately. Forgejo
continues as the internal collaboration provider for issues, pull requests,
comments, and statuses. The change is that **Clotho owns the user-facing
collaboration API and UI**:

- users stay in `apps/web` for primary workflows;
- the frontend talks only to `clotho-api-gateway`;
- Forgejo remains behind the GPLv3/API boundary from `collab/README.md` and
  ADR-0003;
- no Forgejo source is modified unless PRD §8 decision #2 is explicitly
  resolved by a human.

This is the product-shell counterpart to Stage 8: agents can now write through
MCP, and humans have the first CLI. Stage 9 makes those capabilities visible
and useful in the web app.

## Why now

The prototype proved the hard infrastructure bets:

- jj-lib VCS engine with real git objects and op log;
- storage dedup measured over S3-compatible storage;
- Forgejo adoption without source changes;
- MCP agent identity, scopes, audit, and write tools;
- merge-queue integration with first-class conflicts;
- CCI/compute integration;
- API gateway and basic web app.

The product gap is now the web experience. `apps/web` currently supports repo
listing, file browsing, commit/op-log display, PR list/detail, structured diff,
and agent presence. That is enough to prove the architecture, but not enough to
feel like Clotho is the collaboration platform.

The vision spec's product targets are explicit:

- "Vercel-simple": `clotho init` to a live, agent-ready repo;
- "Cloudflare-robust": simple surface over real primitives;
- beautiful, keyboard-driven, collaborative frontend;
- human and agent presence in the same repo experience.

Stage 9 should move the web app toward that target.

## Goals

1. **Make Clotho the primary collaboration UI.** Users should not need to open
   Forgejo for ordinary issue, PR, review, status, or repo browsing workflows.
2. **Hide Forgejo as an implementation detail.** The API gateway should expose
   Clotho-native collaboration endpoints, even when the backing provider is
   Forgejo.
3. **Mature the repo shell.** The repo page should look and behave like a real
   product workspace: code, PRs, issues, checks, agents, storage, insights, and
   settings.
4. **Expose agent-native value.** Agent sessions, audited writes, conflicts,
   checkpoints, and structured diffs should be first-class product surfaces.
5. **Preserve boundaries.** Do not modify Forgejo source, change licenses, or
   hardcode collaboration provider assumptions into the frontend.

## Non-goals

- Do not replace Forgejo as the collaboration data store in Stage 9.
- Do not fork or patch Forgejo source.
- Do not resolve Clotho's top-level license decision.
- Do not add billing, hosted auth, org billing, or enterprise policy controls.
- Do not implement full real-time collaboration infrastructure; polling is
  acceptable where Stage 6 already established it.
- Do not add a marketing landing page inside `apps/web`; this is the product
  app, not the site.

## Proposed architecture

### Collaboration facade

Add a Clotho-owned collaboration facade to `clotho-api-gateway`.

Initial endpoints should cover:

```text
GET    /api/v1/repos/{repo}/issues
POST   /api/v1/repos/{repo}/issues
GET    /api/v1/repos/{repo}/issues/{number}
POST   /api/v1/repos/{repo}/issues/{number}/comments

POST   /api/v1/repos/{repo}/pulls
POST   /api/v1/repos/{repo}/pulls/{number}/comments
POST   /api/v1/repos/{repo}/pulls/{number}/reviews
POST   /api/v1/repos/{repo}/pulls/{number}/merge

GET    /api/v1/repos/{repo}/commits/{sha}/statuses
GET    /api/v1/repos/{repo}/branches
```

Forgejo remains the first provider implementation. The REST shapes returned by
the gateway should be Clotho shapes, not raw Forgejo objects, so a future
provider swap does not rewrite the frontend.

### Web app

`apps/web` should become a full product shell:

- persistent global app frame;
- repo header with clone URL, default branch, latest commit, CI state, conflict
  count, agent activity, and storage summary;
- repo navigation: Code, Pull Requests, Issues, Checks, Agents, Storage,
  Insights, Settings;
- mature code browser with directory hierarchy and README rendering;
- PR review page with changed-file sidebar, structured symbols, line hunks,
  comments/reviews, checks, merge action, and conflict summary;
- native issues list/detail/create/comment flows;
- agent activity page backed by audit/presence data;
- basic settings page for repo metadata and integration links.

Use Cloudflare Kumo components and blocks more substantially than today:
buttons, tabs, menus, tooltips, resource lists, forms, empty states, badges, and
status surfaces. Keep Clotho's Belweave black-and-white design language, but
make it dense, operational, and polished.

### Forgejo dev port

Forgejo should stop occupying host port 3000 in the normal dev workflow.
Recommended first step:

```yaml
ports:
  - "13000:3000"
```

Longer term, remove the host port entirely and keep Forgejo internal-only,
with optional debug exposure via a Compose profile.

## Suggested implementation sequence

1. **ADR-0011:** Record the collaboration facade decision:
   Clotho-owned API/UI, Forgejo as internal provider, no Forgejo source edits,
   and host port moved off 3000.
2. **Forgejo client expansion:** Add issues, comments, reviews, merge, branches,
   and statuses to `crates/clotho-api-gateway/src/forgejo.rs`.
3. **Gateway modules:** Add `issues.rs`, expand `pulls.rs`, add statuses/branches
   endpoints, and map responses into Clotho-owned JSON shapes.
4. **SDK expansion:** Mirror the new API in `packages/sdk-js` with tests.
5. **Web shell:** Refactor `apps/web` into a proper app frame and repo layout.
6. **Issues MVP:** Native issue list/detail/create/comment entirely in Clotho.
7. **PR review MVP:** Comments/reviews/checks/merge action in Clotho, with no
   primary links to Forgejo.
8. **Port cleanup:** Move Forgejo host exposure to `13000` or a debug profile.
9. **Docs:** Add Stage 9 implementation notes to `docs/prd.md` once built.

## Exit condition

Stage 9 is complete when a user can:

- create or open a repo in Clotho;
- browse code, commits, and repo state in the web app;
- create, list, read, and comment on issues in the web app;
- list, review, comment on, inspect checks for, and merge PRs in the web app;
- see agent activity/provenance and conflict state in the repo experience;
- do the above without opening Forgejo or knowing its host port;
- still clone/push with ordinary git against the Forgejo-backed git endpoint;
- pass `just test`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all --check`, and `pnpm turbo build lint typecheck test`.

## Risks

- Forgejo API shapes may leak through unless the gateway maps them carefully.
- Review comments and inline diff positions are easy to get subtly wrong; keep
  the first version small and heavily tested.
- Moving Forgejo's port can break local muscle memory; document the new debug
  URL clearly.
- A large visual refactor can sprawl. Keep product-shell work tied to concrete
  API-backed workflows.

## Open decisions

- PRD §8 #1: Clotho top-level license remains unresolved.
- PRD §8 #2: fork Forgejo vs stay API-level remains unresolved. This proposal
  assumes **stay API-level** for Stage 9.
- Whether to make Forgejo completely internal-only now or first move it to
  `localhost:13000` for debugging.
