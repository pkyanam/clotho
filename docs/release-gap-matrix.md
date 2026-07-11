# Stage 22 release gap matrix

**Audited:** July 11, 2026 through the Stage 22 activity-pagination slice. This
matrix records implementation and evidence separately; “partial” does not
satisfy the public-alpha gate.

Status: **implemented**, **partial**, **missing**, or **evidence gap**.

## REST and OpenAPI

| Gate                           | Status      | Current evidence / precise gap                                                                                                                                                                                                                                          |
| ------------------------------ | ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Complete operation inventory   | Implemented | `pnpm test:contract` parses OpenAPI, resolves refs, and compares all 108 Axum HTTP method/path registrations. It requires unique IDs, mutation request schemas, success schemas, path parameters, and effective alpha auth/error metadata. Pagination remains separate. |
| Versioned stable errors        | Implemented | Envelope version `1` has stable `code`, safe `message`, `request_id`, optional `details`, and `retryable`; catch-all middleware normalizes Axum errors and redacts internal provider/gRPC topology. SDK, CLI, MCP, OpenAPI, tests, and docs preserve the fields.        |
| Cursor pagination and bounds   | Partial     | Global/organization repositories and global activity now use stable page envelopes, opaque keyset cursors, deterministic ordering, and `1..100` limits across REST/OpenAPI/SDK/CLI/MCP/web. Actions has `before`/`next_cursor`; other lists remain limit-only or unbounded. |
| Idempotency keys               | Missing     | Create/start/import/submit routes do not accept or persist a common idempotency key.                                                                                                                                                                                    |
| Request and audit correlation  | Partial     | Every response carries `X-Request-Id`; valid caller IDs are accepted, generated IDs replace invalid values, and structured gateway spans include the same ID. Durable activity/agent-audit schema links are still missing.                                              |
| Conditional reads/writes       | Partial     | Release downloads emit an ETag, but `If-None-Match` and policy `If-Match` behavior are absent.                                                                                                                                                                          |
| Transfer/async conventions     | Partial     | Release byte ranges and bounded previews exist; durable imports/Actions exist. Common operation handles, cancellation, log paging, timeouts, and limit documentation do not.                                                                                            |
| Compatibility/deprecation rule | Partial     | Prose exists in `docs/api.md`; no deprecation headers, API-diff report, or enforced compatibility check.                                                                                                                                                                |
| Structural SDK parity          | Implemented | The verifier inspects 92 SDK REST calls and 72 interfaces, requires canonical endpoint coverage, and compares shared-schema property names, requiredness, and base types. Binary release GET/HEAD maps explicitly to `downloadReleaseFile`.                             |

## CLI

| Gate                                 | Status      | Current evidence / precise gap                                                                                                                                                                         |
| ------------------------------------ | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Frozen/generated command reference   | Missing     | Hand-written `docs/cli.md`; no help-tree snapshot/generator or grammar compatibility test.                                                                                                             |
| stdout/stderr and one-value JSON     | Partial     | Repository page probes verify exactly one JSON envelope on stdout; systematic coverage across command groups and malformed responses is absent.                                                         |
| Stable exit classes                  | Implemented | CLI maps usage/auth/permission/conflict/not-found/retryable/internal to exits `2`–`7`/`1`; live probes verified auth `3`, conflict `5`, not-found `6`, and unavailable `7` with request IDs on stderr. |
| Non-interactive semantics            | Partial     | Destructive repo delete has `--yes`; no common timeout, cancellation, idempotency, or stdin-secret flow.                                                                                               |
| Named contexts + OS credential store | Missing     | Only flags and environment variables exist.                                                                                                                                                            |
| Completions/version/release binaries | Missing     | No completion generation, version command, signed binaries, checksums, or update policy.                                                                                                               |
| Disposable gateway command coverage  | Missing     | Argument unit tests exist only indirectly; no command-group fixture covering auth expiry, policy, malformed JSON, unavailability, and Ctrl-C.                                                          |

## MCP and agent handoff

| Gate                                | Status             | Current evidence / precise gap                                                                                                                                                                                         |
| ----------------------------------- | ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Versioned tool contracts            | Missing            | JSON schemas are generated by rmcp but have no Clotho schema version, stability, side-effect, idempotency, or error metadata.                                                                                          |
| Operations/progress/cancellation    | Missing            | Imports and Actions return REST records, but MCP has no typed common operation handle, cancellation, or bounded log cursor.                                                                                            |
| Capability classes                  | Partial            | Per-tool scopes are enforced and `tools/list` is now filtered to the token; read/propose/mutate/execute/destructive capability classes do not exist.                                                                   |
| MCP↔REST equivalence tests          | Partial            | Live tests compare REST/MCP missing-file errors and now require exact repository page-envelope equivalence at `limit=1`. Broad result/error structural comparisons for every tool family are absent.                  |
| Prompt-injection authority boundary | Implemented (docs) | `docs/mcp.md` and root `AGENTS.md` state imported/repository text and logs are untrusted; dedicated enforcement tests remain an evidence gap.                                                                          |
| Request/audit correlation           | Partial            | REST-backed MCP failures preserve request IDs and every call is audited by agent/token/tool/repo/outcome. The audit row still lacks request ID, originating session, durable operation, and final async outcome links. |
| Public agent bootstrap              | Partial            | Root `AGENTS.md`, `docs/handoff.md`, and `just bootstrap` now exist; a least-privileged public-surface demo without admin/internal setup is not yet automated.                                                         |

## Authentication and authorization

| Gate                                   | Status       | Current evidence / precise gap                                                                                                                                          |
| -------------------------------------- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Route/tool permission matrix           | Partial      | Human role checks and scoped-agent tests exist; no complete route×identity matrix or deny-by-default test inventory.                                                    |
| Tenant isolation                       | Evidence gap | Repo permission tests cover selected routes; artifacts, releases, imports, logs, secrets, providers, audits, indirect IDs, and timing behavior lack a systematic suite. |
| Threat model                           | Missing      | No published threat model covering token theft, confused deputy, SSRF, traversal, model metadata, webhook replay, or sandbox escape.                                    |
| Rotation/replay/redaction/audit export | Partial      | Human/agent expiry and revoke exist; webhook HMAC exists. Secret-master rotation, replay defense, systematic redaction, and incident-safe audit export do not.          |
| Security/supply-chain CI               | Missing      | No complete dependency, container, license, secret, SAST, SBOM, provenance, or reporting-channel gate.                                                                  |

## Data durability and operations

| Gate                                    | Status  | Current evidence / precise gap                                                                                                                                                         |
| --------------------------------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Versioned migrations and upgrade/repair | Partial | SQLx migrations `1001`–`1015` and agent migration `0001` exist; previous-release upgrade, rollback/forward-repair, and realistic-volume tests do not.                                  |
| Complete backup/restore                 | Missing | No automated bundle covering Postgres, Git/VCS, Arachne/object data, and the secrets master key; no destructive restore drill.                                                         |
| Dependency recovery/reconciliation      | Partial | Actions/import leases recover across restarts and Arachne restart is tested; component-outage consistency and observable reconciliation are incomplete.                                |
| Health/readiness/doctor                 | Partial | Service liveness exists. `just bootstrap`/`just doctor [--json] [--stack]` now provide read-only dependency and surface diagnostics; dependency-aware readiness endpoints are missing. |
| Resource bounds/backpressure            | Partial | Upload body, preview, import, Action timeout, repository pages, and the indexed activity query are bounded; queues, logs, retries, other DB queries, and all uploads lack a reviewed common policy. |
| Supported operations envelope           | Missing | Minimum Docker/Compose resources, volume ownership, upgrades, clean uninstall, and support bounds are not published as a release guide.                                                |

## Web and accessibility

| Gate                             | Status  | Current evidence / precise gap                                                                                                                                                                                                                     |
| -------------------------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| WCAG 2.2 AA both themes          | Partial | Dark/light semantic roles now provide distinct surface tiers and automated 4.5:1 meaningful-text plus 3:1 control/focus contrast checks. Shared shell/dashboard browser review exists; journey-wide keyboard, status, and component audits remain. |
| Explicit async/error states      | Partial | Several pages have empty/error UI; no audited loading/partial/stale/unavailable/denied matrix across journeys.                                                                                                                                     |
| Public-alpha Playwright journeys | Missing | No Playwright suite for first boot, repo, import, release, Action, collab, agent, provider, and recovery errors.                                                                                                                                   |
| Responsive/long-value behavior   | Partial | Dashboard and repository list were reviewed at desktop and 320 px; no horizontal overflow, long repository names truncate with a full-value title, and repository paging works through the GUI. Import/Actions/collab/settings route evidence remains. |

## Packaging and project hygiene

| Gate                                   | Status  | Current evidence / precise gap                                                                                                                            |
| -------------------------------------- | ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Public brand/screenshots               | Missing | `logo-placeholder.svg` and README screenshot placeholder remain.                                                                                          |
| Community/security/support files       | Missing | `CONTRIBUTING.md`, `SECURITY.md`, code of conduct, support/governance, templates, changelog, and release notes are absent.                                |
| License/boundary/reproducibility audit | Partial | Apache-2.0 license and unmodified pinned Forgejo boundary are documented; notices, headers, generated ownership, and clean-clone audit remain incomplete. |
| Signed release artifacts               | Missing | No signed multi-platform CLI, pinned container bundle, checksums, SBOM, or provenance attestation.                                                        |

## Agent-ready repository contract

| Gate                              | Status             | Current evidence / precise gap                                                                                                        |
| --------------------------------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| Root `AGENTS.md`                  | Implemented        | Architecture invariants, forbidden shortcuts, commands, verification matrix, and safe Git/Docker rules are present.                   |
| Current handoff                   | Implemented        | `docs/handoff.md` records Stage 22 state and next bounded acceptance.                                                                 |
| Machine capability document       | Missing            | No versioned capability document served by both REST and MCP.                                                                         |
| One diagnostic/bootstrap command  | Implemented        | `just bootstrap`; `just doctor --json --stack` emits one JSON value and actionable fixes without mutation or network-provider access. |
| Deterministic disposable fixtures | Partial            | Tests use unique names and real services; no unified disposable org/repo fixture or cleanup policy.                                   |
| Generated artifact ownership      | Implemented (docs) | Root `AGENTS.md` records OpenAPI/SDK/protobuf/token ownership; CI enforcement remains part of structural parity.                      |
| PR completion checklist           | Missing            | No pull-request template linked to a “do not claim complete unless” checklist.                                                        |

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
