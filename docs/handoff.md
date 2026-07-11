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

No live Daytona, Box, ComputeSDK upstream, managed Clerk, private/gated Hub,
or Tailscale credential test has been run in this slice.

## Next bounded acceptance test

After this slice is green and pushed, implement the versioned REST error
envelope and request ID middleware as the next Stage 22 blocker. Acceptance:

1. Every gateway-generated 4xx/5xx response has stable `code`, safe `message`,
   `request_id`, optional structured `details`, and explicit retry metadata.
2. The same request ID is returned in `X-Request-Id`, accepted from a valid
   inbound header where safe, and recorded in structured logs/audit links.
3. OpenAPI declares the envelope and common errors; SDK preserves all fields;
   CLI maps error codes to stable exit classes; MCP preserves code and request
   correlation for REST-backed tools.
4. Unit, contract, Docker HTTP, and restart probes pass without exposing
   internal topology or secret values.

Do not broaden that slice into pagination, idempotency, or speculative Stage
23 work until the error/correlation acceptance is complete.
