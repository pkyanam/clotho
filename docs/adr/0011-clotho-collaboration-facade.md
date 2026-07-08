# ADR-0011: Clotho owns the collaboration facade

- **Status:** Accepted
- **Date:** 2026-07-08
- **Deciders:** Clotho core

## Context

Stage 8 made Clotho usable from agents and humans without reaching into
service internals: agents can commit and submit through MCP, and the first
`clotho` CLI talks to the REST edge. The web app still feels like a prototype:
repository browsing and PR review are in Clotho, while normal collaboration
workflows still leak Forgejo links and Forgejo occupies the familiar dev port
`localhost:3000`.

PRD §8 still has two unresolved human decisions: Clotho's top-level license and
whether to fork Forgejo or stay API-level. Stage 9 must not settle either one.
The GPLv3 boundary in `collab/README.md` and ADR-0003 still applies.

## Decision

Clotho's API gateway owns the user-facing collaboration API contract. Forgejo
remains the first internal collaboration provider for issues, pull requests,
comments, reviews, merge actions, branches, and commit statuses, but the web app
and CLI-facing clients consume Clotho-owned JSON shapes under
`/api/v1/repos/...`.

The Stage 9 facade adds native edge routes for:

- issues: list, create, read, and comment;
- pull requests: create, comment, review, and merge;
- repository status data: branch list and commit statuses.

The web app must not send users to Forgejo for primary collaboration workflows.
Forgejo can remain reachable for debugging, but it is an implementation detail,
not the product shell.

Forgejo source remains unmodified. Stage 9 continues the API-level integration
from ADR-0003; any Forgejo source patch still requires an explicit human
decision for PRD §8 #2.

In the development stack, Forgejo moves off host port `3000` to
`localhost:13000`. The internal service URL remains `http://forgejo:3000`, so
containers and the gateway do not change. CI may continue to use an ephemeral
Forgejo on `localhost:3000`; that is test infrastructure, not the normal dev
product port.

## Consequences

- `apps/web` can become the primary GitHub/GitLab replacement surface while
  Forgejo stays behind the gateway.
- The facade prevents raw Forgejo API shapes from becoming the frontend
  contract, which keeps a later provider swap possible.
- The GPLv3 boundary remains clean: Clotho calls Forgejo over HTTP and does not
  vendor, link, or patch Forgejo code.
- Local debugging still has a Forgejo URL (`http://localhost:13000`), but the
  normal user path is the Clotho web app on `:3100` and API on `:8080`.
- PRD §8 #1 and #2 remain open human decisions.
