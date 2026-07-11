# Stage 22 public-alpha closure evidence

**Date:** July 11, 2026
**Disposition:** public-alpha gate closed; no production or HA claim

## New closure controls

- Membership-filtered human user/org directories and concealed foreign rosters.
- One versioned capability document served identically by REST and MCP gateways.
- Complete checksummed backup of both databases and six durable volumes.
- Destructive restore verification in disposable Postgres and volume resources.
- Clean recursive clone, zero-`.env` bootstrap, frozen install, and contract check.
- Security workflow for Rust/JS advisories, history secret scanning, Semgrep SAST,
  Trivy vulnerability/license/misconfiguration scanning, and SARIF upload.
- Alpha-tag workflow for three CLI targets, SHA-256 checksums, SPDX SBOM,
  keyless provenance attestations, and prerelease publication.
- Non-root ComputeSDK bridge runtime after the local image audit found it.
- Verified public-alpha dashboard capture embedded in the README.

## Verification

- Full host baseline: `cargo fmt --all --check`, workspace clippy with warnings
  denied, all workspace tests, JS typecheck/lint/test, and production builds.
- Contract: 109 OpenAPI/Axum operations, 93 SDK calls, 75 SDK interfaces.
- Docker: rebuilt gateways and dependencies without deleting volumes; stack and
  REST/web diagnostics ready.
- Live: REST and MCP capability JSON compared byte-for-byte; collaboration,
  scoped-agent MCP, and release-mode storage/restart suites passed.
- Recovery: live-data backup and isolated destructive restore passed for both
  databases and all durable volumes.
- Supply chain: `pnpm audit` found no high/critical advisory; Gitleaks scanned 87
  commits with no leaks after documented false-positive allowlisting; Trivy
  reported zero high/critical Cargo/pnpm vulnerabilities and exposed one
  non-root-container defect, which was corrected.
- Workflow syntax: new security and release workflows passed `actionlint`.

## Credential-gated checks

Live Daytona, Box, ComputeSDK upstream, managed Clerk, private/gated Hugging
Face, and Tailscale credentials were not available. Their unconfigured paths
fail closed; rerunning them is an explicit per-release limitation owned in
`docs/known-limitations.md`.

Every other incomplete beta or release-event promise is dated, impact-scoped,
and owner-assigned in that same register, satisfying the Stage 22 alpha
definition without representing those promises as implemented.
