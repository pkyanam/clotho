# Stage 22 release gap matrix

**Audited:** July 11, 2026 through the Stage 22 tenant-directory authorization
slice. This matrix records implementation and evidence
separately; “partial” does not satisfy the public-alpha gate.

Status: **implemented**, **partial**, **missing**, or **evidence gap**.

**Stage 22 disposition:** closed for public alpha on July 11, 2026. Rows that
remain partial or release-event-dependent are explicitly accepted, dated, and
owned in `known-limitations.md`; none is an unregistered Stage 22 blocker.

## REST and OpenAPI

| Gate                           | Status      | Current evidence / precise gap                                                                                                                                                                                                                                                                                                       |
| ------------------------------ | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Complete operation inventory   | Implemented | `pnpm test:contract` parses OpenAPI, resolves refs, and compares all 108 Axum HTTP method/path registrations. It requires unique IDs, mutation request schemas, success schemas, path parameters, and effective alpha auth/error metadata. Pagination remains separate.                                                              |
| Versioned stable errors        | Implemented | Envelope version `1` has stable `code`, safe `message`, `request_id`, optional `details`, and `retryable`; catch-all middleware normalizes Axum errors and redacts internal provider/gRPC topology. SDK, CLI, MCP, OpenAPI, tests, and docs preserve the fields.                                                                     |
| Cursor pagination and bounds   | Partial     | Global/organization repositories and global activity now use stable page envelopes, opaque keyset cursors, deterministic ordering, and `1..100` limits across REST/OpenAPI/SDK/CLI/MCP/web. Actions has `before`/`next_cursor`; other lists remain limit-only or unbounded.                                                          |
| Idempotency keys               | Partial     | Manual Action starts now persist a 24-hour, org+principal-scoped key and exact `202` response atomically with the run/log. Sequential and concurrent retries replay one run; changed intent fails `409 idempotency_conflict`. OpenAPI/SDK/CLI/MCP/web expose the contract; other retryable creates/imports/submits remain uncovered. |
| Request and audit correlation  | Partial     | Every response carries `X-Request-Id`; valid caller IDs are accepted, generated IDs replace invalid values, and structured gateway spans include the same ID. Durable activity/agent-audit schema links are still missing.                                                                                                           |
| Conditional reads/writes       | Partial     | Release downloads emit an ETag, but `If-None-Match` and policy `If-Match` behavior are absent.                                                                                                                                                                                                                                       |
| Transfer/async conventions     | Partial     | Release byte ranges and bounded previews exist; durable imports/Actions exist. Common operation handles, cancellation, log paging, timeouts, and limit documentation do not.                                                                                                                                                         |
| Compatibility/deprecation rule | Partial     | Prose exists in `docs/api.md`; no deprecation headers, API-diff report, or enforced compatibility check.                                                                                                                                                                                                                             |
| Structural SDK parity          | Implemented | The verifier inspects 92 SDK REST calls and 74 interfaces, requires canonical endpoint coverage, and compares shared-schema property names, requiredness, and base types. Binary release GET/HEAD maps explicitly to `downloadReleaseFile`.                                                                                          |

## CLI

| Gate                                 | Status      | Current evidence / precise gap                                                                                                                                                                                                                                 |
| ------------------------------------ | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Frozen/generated command reference   | Accepted alpha limitation | Hand-written `docs/cli.md`; generated reference/grammar freeze moves to the beta CLI usability gate. |
| stdout/stderr and one-value JSON     | Partial     | Repository page probes verify exactly one JSON envelope on stdout; systematic coverage across command groups and malformed responses is absent.                                                                                                                |
| Stable exit classes                  | Implemented | CLI maps usage/auth/permission/conflict/not-found/retryable/internal to exits `2`–`7`/`1`; live probes verified auth `3`, conflict `5`, not-found `6`, and unavailable `7` with request IDs on stderr.                                                         |
| Non-interactive semantics            | Partial     | Destructive repo delete's advertised `--yes` grammar is executable/live-proven; `actions run --idempotency-key` provides the first persisted retry control. Common timeout, cancellation, idempotency on other mutations, and stdin-secret flow remain absent. |
| Named contexts + OS credential store | Accepted alpha limitation | Alpha uses flags/environment with external credential managers; named contexts/keychain support is a beta gate. |
| Completions/version/release binaries | Accepted first-tag limitation | The tag workflow builds three targets with checksums and keyless attestations; completions/update UX remains a beta gate. |
| Disposable gateway command coverage  | Accepted alpha limitation | Focused JSON/idempotency/error tests exist; exhaustive command-group/Ctrl-C coverage is owner-assigned for beta. |

## MCP and agent handoff

| Gate                                | Status             | Current evidence / precise gap                                                                                                                                                                                                                                   |
| ----------------------------------- | ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Versioned tool contracts            | Accepted alpha limitation | Schemas remain explicitly alpha; version/stability metadata freezes at beta. |
| Operations/progress/cancellation    | Accepted alpha limitation | Durable records exist, while common handles/cancellation remain operation-specific until beta. |
| Capability classes                  | Partial            | Per-tool scopes are enforced and `tools/list` is now filtered to the token; read/propose/mutate/execute/destructive capability classes do not exist.                                                                                                             |
| MCP↔REST equivalence tests          | Partial            | Live tests under required auth compare REST/MCP missing-file errors, repository/activity pages and Action replay/conflict using the same agent bearer; direct foreign/revoked probes prove 404/401. Broad structural comparison for every tool family is absent. |
| Prompt-injection authority boundary | Implemented (docs) | `docs/mcp.md` and root `AGENTS.md` state imported/repository text and logs are untrusted; dedicated enforcement tests remain an evidence gap.                                                                                                                    |
| Request/audit correlation           | Partial            | REST-backed MCP failures preserve request IDs and every call is audited by agent/token/tool/repo/outcome. The audit row still lacks request ID, originating session, durable operation, and final async outcome links.                                           |
| Public agent bootstrap              | Partial            | Root `AGENTS.md`, `docs/handoff.md`, and `just bootstrap` now exist; a least-privileged public-surface demo without admin/internal setup is not yet automated.                                                                                                   |

## Authentication and authorization

| Gate                                   | Status       | Current evidence / precise gap                                                                                                                                                                                                                                                              |
| -------------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Route/tool permission matrix           | Partial      | A two-human/two-org suite covers 32 repository read shapes, lists/activity before pagination, secret metadata, scoped REST-backed MCP tools, membership-filtered user/org directories, and concealed foreign org rosters under required auth. Human provider metadata, indirect ids, and a complete deny-by-default inventory remain. |
| Tenant isolation                       | Evidence gap | Repository reads now gate before VCS/storage/provider calls and scoped agents are revalidated at REST. Exhaustive artifacts/jobs/caches/storage prefixes/webhooks/provider calls/indirect IDs/timing and typed tenant-context evidence belongs to the still-blocked Stage 23 gate.          |
| Threat model                           | Implemented  | `docs/threat-model.md` covers assets, trust boundaries, token theft, confused deputy, SSRF, traversal, untrusted content, webhook replay, sandbox escape, recovery, supply chain, and explicit remaining work.                                                                              |
| Rotation/replay/redaction/audit export | Partial      | Human/agent expiry and revoke exist. Forgejo webhooks now require exact-body HMAC plus hashed, atomic, 24-hour delivery reservations; exact/concurrent retries collapse and changed payloads conflict. Secret-master rotation, systematic redaction, and incident-safe audit export remain. |
| Security/supply-chain CI               | Implemented (workflow) | `.github/workflows/security.yml` gates Rust/JS advisories, secrets, SAST, filesystem vulnerabilities, licenses, and misconfiguration. The tag workflow emits an SPDX SBOM, checksums, and keyless build provenance; first-run artifacts are a first-tag limitation. |

## Data durability and operations

| Gate                                    | Status  | Current evidence / precise gap                                                                                                                                                                                                                        |
| --------------------------------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Versioned migrations and upgrade/repair | Partial | SQLx migrations `1001`–`1018` and agent migration `0001` exist; `1017` adds common idempotency records and `1018` hashed webhook-delivery reservations. Previous-release upgrade, rollback/forward-repair, and realistic-volume tests do not.         |
| Complete backup/restore                 | Implemented | `just backup` captures both Postgres databases and six durable volumes with checksums and key-presence enforcement. `just restore-drill` destructively restores into disposable resources; the July 11 live-data drill passed after exposing and correcting two recovery defects. |
| Dependency recovery/reconciliation      | Partial | Actions/import leases recover across restarts and Arachne restart is tested; component-outage consistency and observable reconciliation are incomplete.                                                                                               |
| Health/readiness/doctor                 | Partial | Service liveness exists. `just bootstrap`/`just doctor [--json] [--stack]` now provide read-only dependency and surface diagnostics; dependency-aware readiness endpoints are missing.                                                                |
| Resource bounds/backpressure            | Partial | Upload body, preview, import, Action timeout, repository/activity pages, and Action idempotency keys/responses/cleanup batches are bounded; queues, logs, retries, other DB queries, and all uploads lack a reviewed common policy.                   |
| Supported operations envelope           | Partial | `SUPPORT.md` publishes the public-alpha Compose resource/test envelope, support window, volume-destructive uninstall warning, and forward-only migration posture. Complete backup/upgrade instructions and verified ownership variants remain absent. |

## Web and accessibility

| Gate                             | Status  | Current evidence / precise gap                                                                                                                                                                                                                         |
| -------------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| WCAG 2.2 AA both themes          | Partial | Dark/light semantic roles now provide distinct surface tiers and automated 4.5:1 meaningful-text plus 3:1 control/focus contrast checks. Shared shell/dashboard browser review exists; journey-wide keyboard, status, and component audits remain.     |
| Explicit async/error states      | Partial | Several pages have empty/error UI; no audited loading/partial/stale/unavailable/denied matrix across journeys.                                                                                                                                         |
| Public-alpha Playwright journeys | Accepted alpha limitation | Real browser evidence covers both themes, dashboard/repository desktop and mobile; journey-wide Playwright automation is owner-assigned before beta. |
| Responsive/long-value behavior   | Partial | Dashboard and repository list were reviewed at desktop and 320 px; no horizontal overflow, long repository names truncate with a full-value title, and repository paging works through the GUI. Import/Actions/collab/settings route evidence remains. |

## Packaging and project hygiene

| Gate                                   | Status      | Current evidence / precise gap                                                                                                                                                    |
| -------------------------------------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Public brand/screenshots               | Implemented | The non-placeholder Clotho logo and privacy-safe verified light-theme public-alpha dashboard capture are embedded in the README; paired light/dark evidence remains under `docs/evidence/stage22-web`. |
| Community/security/support files       | Implemented | Contributing, private security reporting, code of conduct, support, governance, changelog, issue/PR templates, known limitations, and release checklist are present and linked.   |
| License/boundary/reproducibility audit | Partial     | Apache-2.0, `THIRD_PARTY_NOTICES.md`, and the unmodified pinned Forgejo boundary are documented. A recursive clean-clone bootstrap/install/format/109-operation contract audit passed July 11; generated license/SBOM evidence runs in the security/release workflows. |
| Signed release artifacts               | Accepted first-tag limitation | The alpha-tag workflow builds three CLI targets, checksums them, emits an SPDX SBOM, applies GitHub keyless attestations, and publishes a prerelease. No tag/registry action was authorized in Stage 22; container digest publication remains a first-tag limitation. |

## Agent-ready repository contract

| Gate                              | Status             | Current evidence / precise gap                                                                                                                               |
| --------------------------------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Root `AGENTS.md`                  | Implemented        | Architecture invariants, forbidden shortcuts, commands, verification matrix, and safe Git/Docker rules are present.                                          |
| Current handoff                   | Implemented        | `docs/handoff.md` records Stage 22 state and next bounded acceptance.                                                                                        |
| Machine capability document       | Implemented        | `docs/capabilities.json` is embedded and served by REST `/api/v1/capabilities` and the MCP gateway `/capabilities`; OpenAPI and SDK expose the canonical REST operation. |
| One diagnostic/bootstrap command  | Implemented        | `just bootstrap`; `just doctor --json --stack` emits one JSON value and actionable fixes without mutation or network-provider access.                        |
| Deterministic disposable fixtures | Partial            | Tests use unique names and real services; no unified disposable org/repo fixture or cleanup policy.                                                          |
| Generated artifact ownership      | Implemented (docs) | Root `AGENTS.md` records OpenAPI/SDK/protobuf/token ownership; CI enforcement remains part of structural parity.                                             |
| PR completion checklist           | Implemented        | `.github/pull_request_template.md` requires surface parity, exact evidence/skips, bounds, durability, browser coverage, docs, risks, and release-gate truth. |

## Baseline evidence

On July 11, 2026 the documented host baseline passed: formatting, workspace
clippy, all Rust tests, JS typecheck, lint, tests, Docker build/start, and
Compose `ps`. Live `just test-storage` passed. Live collaboration and MCP tests
initially exposed Forgejo-null decoding and unscoped tool discovery. After the
affected containers were rebuilt non-destructively, `just test-collab` and
`just test-agent` both passed against the live stack; the existing volumes
survived the restart.

Credential-gated Daytona, Box, ComputeSDK upstream, managed Clerk, private/
gated Hugging Face, and live Tailscale operations were not exercised and must
not be counted as passing release evidence.

The final Stage 22 closure evidence is recorded in
[`evidence/stage22-closure.md`](evidence/stage22-closure.md). The latest slice rebuilt/recreated the Rust service containers without removing
volumes, passed `just test-collab` and `just test-agent`, then repeated the MCP
suite with `CLOTHO_AUTH_REQUIRED=true` and a temporary human setup token that
was revoked afterward. The same agent bearer drove direct REST equivalence;
foreign scope returned concealed `404` and a revoked token returned `401`.
A disposable live webhook delivery returned `202`, exact replay `200`, and a
changed payload under the same id `409`; repository deletion cascaded the
reservation. The served OpenAPI hash matched the checked-in document, migration
1018 was applied, owned fixtures were absent, and local open auth was restored.
