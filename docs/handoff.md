# Current handoff — Stage 22 public alpha

**Updated:** July 11, 2026

## Release target

Clotho is hardening for a credible public alpha. Stage 22 is active. The product
owner approved revised Stages 23–29, but they remain implementation-blocked until
the public-alpha gate is materially closed.

The current evidence-backed gap inventory is
[`release-gap-matrix.md`](release-gap-matrix.md). The governing checklist is
[`release-readiness.md`](release-readiness.md).

## Completed in the active slice

- Added the root agent contract (`AGENTS.md`).
- Added read-only deterministic diagnostics: `just bootstrap`, `just doctor
--json`, and `just doctor --stack`.
- Made MCP `tools/list` reflect each token's allowed-tool scope; call-time
  enforcement and audit remain unchanged.
- Normalized Forgejo's `null` empty-assignee representation at the internal
  provider boundary so Clotho's issue schema remains stable.
- Repaired invalid OpenAPI YAML indentation and an unresolved schema reference.
- Added this current handoff and the implemented/missing/evidence gap matrix.
- Added stable REST error envelope version `1`, safe topology redaction, and
  `X-Request-Id` propagation/generation for every success and error response.
- Preserved error codes and correlation through the JavaScript SDK, CLI, and
  REST-backed MCP tools; froze CLI exit classes `1`–`7`.
- Corrected missing-file semantics from internal VCS through REST to `404 /
not_found` and added a live MCP↔REST error-equivalence assertion.
- Replaced the monotone black/white console palette with theme-specific
  semantic roles for canvas, three surface tiers, meaningful text, controls,
  focus, accent, and status in `@clotho/ui`.
- Added deterministic WCAG contrast tests for both themes; restructured the
  shared shell/dashboard with explicit active navigation, raised panels,
  section markers, metric hierarchy, non-color row cues, and a bounded
  long-name-safe organization rail.
- Added `pnpm test:contract`: parsed 108-operation Axum/OpenAPI method-path
  equality, reference/operation/request/success/path metadata checks, canonical
  SDK endpoint coverage, and shared schema property/requiredness/type parity.
- Corrected secret-detail parameter naming and missing Hugging Face, binary,
  and webhook OpenAPI schemas; added deterministic JSON inventory and API diff
  evidence under `docs/evidence/stage22-api-contract.md`.
- Added bounded opaque cursor pagination to global and organization repository
  lists across REST/OpenAPI/SDK/CLI/MCP/web, with a `1..100` maximum, stable
  keyset ordering, explicit page envelopes, and guarded SDK compatibility
  helpers.
- Added live MCP↔REST page equivalence and browser pagination evidence. The GUI
  check exposed provider repositories with empty timestamps; cursor decoding
  now accepts that legitimate sort key and has a regression test.
- Added bounded opaque keyset pagination to the global activity feed across
  REST/OpenAPI/SDK/CLI/MCP/web. The `1..100` query orders by immutable
  `(created_at, id)`, uses an additive Postgres index, returns explicit page
  envelopes, and rejects malformed cursors instead of restarting traversal.

## Baseline state

The host baseline passes:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `pnpm typecheck`, `pnpm lint`, `pnpm test`
- `docker compose -f docker-compose.dev.yml up -d --build`
- `docker compose -f docker-compose.dev.yml ps`
- `just test-storage`

Before the fixes, live `just test-collab` failed on Forgejo's `assignees: null`
response and `just test-agent` failed because discovery advertised tools
outside the token scope. Both affected images were rebuilt and containers
recreated without removing volumes. `just test-collab`, `just test-agent`, and
`just doctor --json --stack` then passed; the live OpenAPI SHA-256 matched the
checked-in document.

For the error/correlation slice, live HTTP probes verified caller-supplied and
generated request IDs on 200/400/404 responses, catch-all JSON normalization,
and topology-safe provider errors. CLI probes verified exit `2` (usage), `3`
(auth), `5` (conflict), `6` (not found), and `7` (unavailable). A live MCP test
compared the REST and MCP missing-file code and required the REST request ID in
the tool error. VCS, agent gateway, and API gateway were restarted; an existing
repository and the stable error contract remained available afterward.
With the internal collaboration provider stopped, its issue route returned
`502 upstream_unavailable`, `retryable: true`, the caller's request ID, and no
provider URL/topology; the provider was restarted and passed its health probe.

For the console slice, `@clotho/ui` contrast tests, web typecheck, lint, and
production build passed. The rebuilt Docker console was reviewed in dark and
light themes at 1280 px and at 320 px. Theme selection persisted across route
navigation, semantic tiers resolved to distinct computed colors, desktop had
no horizontal overflow, and the mobile overflow exposed by long generated
repository names was corrected in the source before final verification.

For the API-structure slice, the full Rust and JavaScript host baselines passed
with the contract verifier reporting 108 OpenAPI/Axum operations, 92 SDK calls,
and 72 SDK interfaces. The API gateway image was rebuilt and restarted without
removing volumes; its served OpenAPI SHA-256 matched the checked-in file, the
renamed secret-detail route returned the stable `404 not_found` envelope, and
`just doctor --json --stack` remained ready.

No live Daytona, Box, ComputeSDK upstream, managed Clerk, private/gated Hub,
or Tailscale credential test has been run in this slice.

For the repository-pagination slice, focused cursor, CLI, SDK, and agent tests
passed. Docker API, MCP, and web images were rebuilt without removing volumes.
Live HTTP traversed non-overlapping pages, rejected `limit=0` with versioned
`invalid_request`, and reused a cursor after an API restart. CLI JSON emitted
one bounded envelope; `just test-agent` compared MCP and REST pages. The web
repository page and visible next-page link were exercised in the browser at
desktop and 320 px with no horizontal overflow.

Activity-pagination verification is recorded in
[`evidence/stage22-activity-pagination.md`](evidence/stage22-activity-pagination.md).
The full Rust/JavaScript baseline and all three live stack suites passed. The
rebuilt API, MCP, and web surfaces traversed non-overlapping activity pages; a
pre-restart cursor remained valid after an API restart; CLI JSON and live
MCP↔REST envelopes matched; and browser review passed at 1280 px and 320 px.

## Next bounded acceptance test

Add common persisted idempotency keys to one retryable create/start operation,
or extend the bounded cursor convention to the next smallest high-volume list
family if that is smaller. Preserve REST-first parity and do not start Stage 23
until the Stage 22 gate is materially closed.

After Stage 22 is materially closed, begin Stage 23—not the previous Capsule
plan. Stage 23 establishes production identity, authorization, and tenant
isolation; Stage 24 establishes hosted/self-host production profiles and elastic
workload cells. Capsules move to Stage 25, Lachesis plus Signals to Stage 26,
Compute Bindings to Stage 27, lazy/protocol work to Stage 28, and the provider/
lifecycle/federation kit to Stage 29. See ADR-0023 and ADR-0024.
