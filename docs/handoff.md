# Current handoff — Stage 22 public alpha

**Updated:** July 11, 2026

## Release target

Clotho is hardening for a credible public alpha. Stage 22 is active; Stages
23–27 are out of scope until the public-alpha gate is materially closed.

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
and 70 SDK interfaces. The API gateway image was rebuilt and restarted without
removing volumes; its served OpenAPI SHA-256 matched the checked-in file, the
renamed secret-detail route returned the stable `404 not_found` envelope, and
`just doctor --json --stack` remained ready.

No live Daytona, Box, ComputeSDK upstream, managed Clerk, private/gated Hub,
or Tailscale credential test has been run in this slice.

## Next bounded acceptance test

After the API-structure slice is green and pushed, implement bounded cursor
pagination for canonical collections as the next Stage 22 blocker. Define one
envelope and maximum-limit policy in REST/OpenAPI/SDK, migrate the smallest
high-volume list family first, and preserve a documented compatibility path
for existing SDK callers. Do not broaden into speculative Stage 23 work.
