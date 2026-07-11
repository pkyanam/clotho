# Stage 22 repository pagination evidence

**Baseline:** `a59c7c3`

**Audited:** July 11, 2026

## Contract

`GET /api/v1/repos` and `GET /api/v1/orgs/{org}/repos` accept an optional
`limit` (`1..100`, default `100`) and opaque `cursor`. Both return
`{ "repos": [...], "next_cursor": string | null }`, ordered by `updated_at`
descending then name ascending. Invalid, empty, or oversized cursors fail with
the stable `400 invalid_request` envelope.

The JavaScript SDK exposes one-page methods and guarded compatibility methods
that follow pages. CLI and MCP expose explicit one-page semantics. The web
repository list requests 50 rows and renders a visible continuation link.

## Automated evidence

- three Rust pagination tests cover stable non-overlapping traversal, bounds,
  invalid cursors, and an emitted cursor whose provider timestamp is empty;
- SDK tests cover URL encoding and guarded two-page aggregation;
- `pnpm test:contract` verifies the shared OpenAPI response shape and all 108
  Axum/OpenAPI operations, 92 SDK calls, and 72 SDK interfaces;
- the live agent test requires `list_repos(limit=1)` to equal the canonical REST
  page envelope exactly.

## Live evidence

- Docker API, MCP, and web images rebuilt and restarted without removing
  volumes;
- HTTP traversed two `limit=1` pages without overlap and rejected `limit=0`
  with `400 invalid_request`, a request ID, and `retryable: false`;
- the CLI emitted exactly one JSON page envelope;
- a cursor issued before an API restart remained usable afterward;
- the browser rendered the structured repository panel, followed the visible
  next-page link, and showed distinct second-page repositories;
- at 320 px the page had `scrollWidth == innerWidth`, repository names
  truncated, and the continuation remained present.

The browser pass caught a real compatibility edge: older/imported Forgejo
repositories may have an empty `updated_at`. The encoder emitted that legitimate
sort key while the decoder initially rejected it. The decoder and regression
test now guarantee that every cursor Clotho emits can be consumed.

No Daytona, Box, ComputeSDK upstream, managed Clerk, private/gated Hugging Face,
or live Tailscale credentials were available or required for this slice.
