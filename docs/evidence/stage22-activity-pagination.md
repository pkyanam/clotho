# Stage 22 activity-pagination evidence

**Date:** July 11, 2026

## Outcome and contract

`GET /api/v1/activity` now returns one bounded page:

```json
{
  "events": [],
  "next_cursor": null
}
```

The default page size is 50 and the accepted range is `1..100`. Ordering is
newest-first by immutable `(created_at, id)`. `next_cursor` is an opaque,
URL-safe versioned token; malformed, empty, oversized, and unsupported cursors
fail with the stable `400 invalid_request` envelope. REST, OpenAPI, the
JavaScript SDK, CLI, REST-backed MCP tool, and web activity page use the same
page envelope.

The compatibility SDK `activity()` helper still returns the events from one
bounded page. New automation should use `activityPage()` so the continuation is
explicit. CLI and MCP also return exactly one page and never traverse in the
background.

## Migration and compatibility

Migration `1016_activity_cursor_index.sql` adds the
`activity_events_created_id_idx` index over `(created_at desc, id desc)`. It
does not rewrite rows or change ownership. The existing Postgres volume was
upgraded in place by the rebuilt gateway, and an index-catalog probe confirmed
the index exists. No rollback is required for application compatibility; an
older gateway ignores the additive index and extra JSON field.

This is an additive API change during the documented `0.x` alpha window. The
previous silent `limit` clamp is intentionally replaced by a stable validation
error. Existing clients that read only `events` continue to work.

## Automated verification

Passed:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck
pnpm lint
pnpm test
pnpm --filter @clotho/web build
pnpm test:contract
just test-collab
just test-agent
just test-storage
```

Contract verification reported 108 OpenAPI operations, 108 Axum operations,
92 SDK calls, and 74 SDK interfaces. Focused tests covered cursor round trips
and rejection, CLI bounds, SDK query/envelope shape, two non-overlapping live
gateway pages, and exact live MCP↔REST equivalence for `limit=1`.

## Docker, HTTP, CLI, browser, and restart evidence

The affected services were rebuilt and recreated without removing volumes:

```sh
docker compose -f docker-compose.dev.yml up -d --build \
  clotho-api-gateway clotho-agent-gateway clotho-web
docker compose -f docker-compose.dev.yml ps
```

All core containers remained running. Live HTTP observed different event ids on
the first and second `limit=1` pages. `limit=0` returned HTTP 400 with
`code=invalid_request`. The executable CLI `--json activity --limit 1` emitted
one valid object containing one event and `next_cursor`. A Postgres catalog
query confirmed migration 1016's index.

The API gateway was then restarted with `docker compose ... restart
clotho-api-gateway`. A cursor issued before the restart returned the next,
non-overlapping event afterward. `just doctor --json --stack` reported the
stack, REST, and web checks ready; its only warning was the expected changed
worktree.

The rebuilt web activity route was reviewed in the in-app browser. At 1280 px,
the first page rendered 50 event rows, exposed one next-page link, navigated to
a second 50-row page, and had no horizontal overflow. At 320 px, the first page
again rendered 50 rows, the next link remained visible, the widest row stayed
within 286 px, and the document width remained exactly 320 px.

## Skipped credential-gated checks

No live Daytona, Box, ComputeSDK upstream, managed Clerk, private/gated Hugging
Face, or Tailscale credential operation was run. They are unrelated to this
collection contract and are not counted as passing evidence.

## Remaining risk

The Stage 22 cursor-pagination gate remains partial: issue, pull, notification,
secret, agent/audit, provider, and other collections still need reviewed bounds
or explicit non-pagination contracts. Idempotency, backup/restore, complete
authorization/threat-model evidence, packaging, project hygiene, and other P0
items also remain open. Stage 23 is therefore still blocked.
