# Changelog

Clotho follows the principles of Keep a Changelog. Until the first tagged
public alpha, entries describe the evolving `main` branch and no compatibility
window is implied beyond the rules in `docs/api.md`.

## Unreleased

### Added

- Complete Axum/OpenAPI route inventory checks and structural JavaScript SDK
  parity checks.
- Versioned safe REST errors, end-to-end request IDs, repository and activity
  cursor pagination, and persisted manual Action-run idempotency.
- Deterministic bootstrap/doctor commands and evidence-backed live test gates
  with bounded fixture cleanup.
- Public-alpha release, production-tenancy, and frontier-roadmap documents.
- Durable Forgejo webhook replay admission and scoped agent reauthorization at
  the canonical REST boundary.

### Changed

- Web color roles and focus/control contrast now meet the automated WCAG AA
  thresholds covered by the design-token suite.
- Bootstrap credentials are provisioned only from an explicit secret-managed
  value and are never printed in service logs.

### Security

- Generated CI credentials are masked and scanned out of captured logs.
- Internal Forgejo token files are created and repaired with owner-only `0600`
  permissions.
- Private repository reads, secret metadata, and REST-backed MCP calls now
  fail closed against Clotho-owned authorization before provider side effects.

### Known limitations

- No tagged binaries or supported upgrade line exists yet.
- Production tenancy, high availability, and signed release artifacts remain
  outside the public-alpha Compose profile.

## Release history

No public release has been tagged. The first alpha entry will include its
source commit, migrations, API diff, artifact digests, recovery drill, skipped
credential-gated checks, and known limitations.
