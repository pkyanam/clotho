# ADR-0010: The clotho CLI uses the api-gateway REST edge

- **Status:** Accepted
- **Date:** 2026-07-08
- **Deciders:** Clotho core

## Context

The vision spec (§5) names the product target: `clotho init` should be the
simple human entry point, while the architecture keeps VCS, storage,
collaboration, agent identity, and compute decoupled behind stable service
boundaries. Stage 8 needs the first real `clotho` binary.

The CLI could talk directly to gRPC services, to MCP, or to the REST edge.
Direct gRPC would expose internal topology to humans. MCP is agent-native and
identity/audit oriented, but not the right default UX for humans. ADR-0007
already made the web app consume only the API gateway.

## Decision

The CLI talks to **`clotho-api-gateway` REST/JSON** by default.

- `clotho init` calls `POST /api/v1/repos`.
- `clotho status`, `clotho log`, and `clotho pr` call the existing read/PR
  endpoints.
- `clotho commit` calls the new edge `POST /api/v1/repos/{repo}/commits`.
- `clotho submit` and `clotho commit --submit` call the new edge
  `POST /api/v1/repos/{repo}/submit`, which routes to the merge queue.

The binary is dependency-light and hand-written for now: manual argument
parsing, `reqwest`, and JSON structs. It never shells out to `git` or `jj`.
The first commit workflow sends explicit text files via `--file`; recursive
working-tree discovery, ignores, binary uploads, and config files are later
product work.

## Consequences

- Humans get one stable endpoint (`CLOTHO_API_URL`, default
  `http://localhost:8080`) matching the web app's boundary.
- The REST edge now owns a small write surface as well as reads. Internal VCS
  and merge-queue gRPC remain private implementation details for human CLI
  workflows.
- This ADR does **not** resolve PRD §8 decisions #1 or #2. The repository
  remains under its existing Apache-2.0 metadata, and Forgejo stays an
  unmodified API-level dependency until a human explicitly changes those
  decisions.
