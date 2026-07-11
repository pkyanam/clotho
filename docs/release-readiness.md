# Public release readiness

**Stage 22 gate disposition (July 11, 2026): closed for public alpha.** Cross-
system evidence is linked from `docs/release-gap-matrix.md`; incomplete beta or
release-event behavior is explicitly owner-accepted in `docs/known-limitations.md`.
This is not a production or HA claim.

**Target:** credible public alpha, then a compatibility-frozen beta.

This is the release gate for Clotho. It is intentionally stricter than “the
demo works”: public users and autonomous agents must be able to install,
understand, operate, recover, and safely extend the platform without access to
the original development conversation.

## Release principles

1. **Clotho is the source of truth.** Public workflows never require a user or
   agent to understand Forgejo, internal service topology, or database tables.
2. **One contract, multiple clients.** REST semantics lead; SDK, CLI, MCP, web,
   Git, and Hub compatibility cannot invent conflicting behavior.
3. **Fail closed, explain clearly.** Auth, unsafe artifacts, private networking,
   provider capability, release verification, and destructive actions fail with
   stable, actionable errors.
4. **No secret ceremony for the default path.** Local startup works without a
   hand-written `.env`; secrets are connected in Clotho or generated safely.
5. **Recovery is a feature.** Backups, migrations, idempotency, resumability,
   and restart behavior are release requirements, not operations footnotes.

## Release tiers

| Tier | Promise | Gate |
|---|---|---|
| Public alpha | Useful for evaluation and non-critical self-hosting | P0 checklist complete; known limitations published |
| Beta | Stable daily workflows and upgrade path | contract compatibility window; restore drills; security review complete |
| Stable | Supported production deployment | SLOs, migrations, HA, key rotation, incident and deprecation policies |

Do not label Clotho production-ready before the stable gate.

## Post-alpha production gates

Stage 22 closes the public-alpha gate; it does not authorize a production or HA
claim. The next two stages are mandatory foundations before frontier expansion:

- **Stage 23 — identity and tenancy:** multi-user organizations, invitations and
  lifecycle, hosted Clerk plus production-self-host OIDC, Clotho-owned RBAC and
  repository grants, typed tenant context on every resource/job/storage/cache/
  audit path, row-level-security defense in depth, and adversarial isolation.
- **Stage 24 — production control plane:** stateless edge replicas, durable
  operations/leases and reconciliation, tenant quotas/fairness/metering,
  autoscaling workload cells, supported Compose and Helm/Kubernetes profiles,
  HA/upgrade/restore/key-rotation evidence, SLOs, observability, signed artifacts,
  and incident procedures.

Hosted and self-hosted Clotho use the same contracts, schemas, migrations, and
conformance suite. Hosted convenience may select managed providers, but must not
create a proprietary authorization or data model. Autoscaling claims apply to
the workload plane only where accepted work is durable and tenant fairness is
proven; stateful services require explicit ownership and recovery designs.

See [ADR-0023](adr/0023-production-tenancy-and-deployment-profiles.md). Signals
and other public community features remain after these foundations; their
privacy and trust semantics are recorded in
[ADR-0024](adr/0024-signals-public-interest-semantics.md).

## P0 — public alpha blockers

### REST and OpenAPI

- Make `docs/openapi.yaml` the complete stable-route inventory, not only a path
  presence check. Every operation needs an `operationId`, auth declaration,
  request/response schema, error responses, examples, pagination behavior, and
  stability label.
- Standardize errors as a versioned envelope with `code`, `message`,
  `request_id`, optional field-level `details`, and a safe retry hint. Preserve
  machine-readable codes across CLI, SDK, and MCP translations.
- Define cursor pagination, filtering, sorting, and maximum page sizes for all
  unbounded collections. Never make clients infer pagination from arrays.
- Accept an idempotency key for every create/start/import/submit operation that
  can be retried after a timeout. Persist the resulting resource or operation.
- Emit request IDs and structured audit correlation IDs end to end.
- Document conditional reads (`ETag`/`If-None-Match`) and optimistic writes
  (`If-Match`) where concurrent humans and agents can overwrite policy.
- Define upload/download limits, byte-range behavior, timeouts, cancellation,
  and asynchronous-operation resources consistently.
- Add a compatibility rule: additive changes within `v1`; removals or semantic
  changes require deprecation headers, a migration note, and a versioned path.
- Generate the JavaScript types/client from the contract or verify every method
  and schema structurally in CI; path-only drift detection is insufficient.

### CLI

- Freeze command grammar and publish a command reference generated from the
  executable help tree.
- Reserve stdout for requested data and stderr for progress/errors. `--json`
  must produce exactly one documented JSON value and no decoration.
- Publish stable exit-code classes: usage, auth, permission, conflict/policy,
  not found, unavailable/retryable, and internal failure.
- Support non-interactive use explicitly: `--yes`, stdin for secret values,
  timeouts, cancellation, idempotency keys, and no hidden prompts in CI.
- Add a config file with named contexts and use the OS credential store for
  tokens. Environment variables and flags remain automation overrides.
- Add shell completions, man-page-quality help, version output, update guidance,
  and platform release binaries with checksums/signatures.
- Test every command group against a disposable gateway fixture, including
  malformed JSON, unavailable services, expired auth, policy blocks, and Ctrl-C.

### MCP and autonomous-agent handoff

- Version every tool schema and document stability, side effects, required
  scopes, idempotency, and error codes. Do not rely on prose-only server hints.
- Return typed operation handles for long-running imports, Actions, releases,
  and submissions. Support progress polling, cancellation where safe, and
  bounded log pagination.
- Separate read, propose, mutate, execute, credential-metadata, and destructive
  capabilities so tokens can express least privilege without enumerating every
  tool forever.
- Keep all collaboration/platform tools behind REST and add contract tests that
  compare MCP results/errors with their REST equivalents.
- Treat repository text, issue comments, model cards, workflow logs, and imported
  metadata as untrusted content. Document prompt-injection boundaries and never
  convert third-party prose into authority.
- Include request/audit IDs in tool results. Every mutation records agent,
  token, repo, operation, originating session, and final outcome.
- Publish an agent bootstrap document that an unfamiliar agent can follow from
  clean clone to scoped token, orientation, change, test, submission, and
  recovery without privileged internal knowledge.

### Authentication and authorization

- Produce a route/tool permission matrix and test deny-by-default behavior for
  anonymous, human, org admin, repo collaborator, and scoped-agent identities.
- Verify tenant isolation at every repo, artifact, release, import, log, secret,
  provider, and audit boundary—including indirect identifiers and timing-safe
  not-found behavior where appropriate.
- Complete threat modeling for token theft, confused deputy paths, SSRF through
  providers/importers, archive traversal, unsafe model metadata, webhook replay,
  and sandbox escape.
- Add token expiry/rotation, secret-key rotation, webhook signing/replay defense,
  redaction tests, and an incident-safe audit export.
- Run dependency, container, license, secret, SAST, and supply-chain scans in CI;
  publish the supported security-reporting channel.

### Data durability and operations

- Version every Postgres migration; test upgrade and rollback/forward-repair
  from the previous supported release with realistic data volumes.
- Document and automate backup/restore for Postgres, Git objects, Arachne/object
  storage, and the secrets master key. Run a destructive restore drill.
- Define consistency and recovery behavior when one backing component is
  unavailable. Reconciliation must be idempotent and observable.
- Add health, readiness, and dependency diagnostics plus `clotho doctor` output
  suitable for both humans and agents.
- Bound queues, uploads, previews, logs, imports, retries, and database queries;
  add backpressure rather than allowing memory/disk exhaustion.
- Publish minimal supported Docker/Compose versions, resource requirements,
  persistent volume ownership, upgrade steps, and clean uninstall behavior.

### Web quality and accessibility

- Meet WCAG 2.2 AA for text, controls, focus indicators, status communication,
  keyboard navigation, landmarks, and reduced motion in both themes.
- Replace visual-state inference with explicit loading, empty, partial, stale,
  unavailable, permission-denied, and failed states.
- Add Playwright coverage for the public-alpha journeys: first boot, repo create,
  import/progress navigation, release, Action, issue/PR, agent provisioning,
  provider connection, and restore-friendly errors.
- Validate responsive behavior at 320 px through wide desktop; long repository,
  organization, branch, artifact, and provider names must wrap or truncate with
  accessible full-value affordances.

See [the web console design note](design/stage13-web-console.md) for the contrast
remediation specification derived from the July dashboard capture.

### Packaging and project hygiene

- Replace the README logo placeholder and add a current, privacy-safe product
  screenshot after the contrast pass.
- Add `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, support policy,
  issue/PR templates, changelog, release notes, and governance/maintainer scope.
- Verify license headers, third-party notices, Forgejo separation, generated
  artifacts, submodule setup, and clean-clone reproducibility.
- Publish signed CLI binaries, container images pinned by digest, SBOMs,
  provenance attestations, checksums, and a tested Compose release bundle.

## Agent-ready repository contract

Before handing Clotho to an autonomous implementation agent, add and maintain:

- a root `AGENTS.md` containing architecture invariants, forbidden shortcuts,
  commands, test matrix, and safe Git/Docker rules;
- `docs/handoff.md` generated from current state: release target, active stage,
  completed work, known failures, and the next bounded acceptance test;
- a machine-readable capability document served by the API and MCP gateway;
- one bootstrap command that verifies dependencies and reports actionable fixes;
- deterministic fixtures and disposable test organizations/repos;
- explicit ownership of generated OpenAPI/SDK/protobuf files;
- a “do not claim complete unless…” checklist linked from pull requests.

An agent should never need raw database access, Forgejo UI access, or an
undocumented environment variable to complete a normal product task.

## Release evidence

Each public release should attach:

- source commit and signed tag;
- changelog and migration notes;
- container and binary digests;
- SBOM and provenance attestation;
- full test summary plus env-gated tests that were skipped;
- backup/restore drill result;
- API diff and deprecation report;
- known limitations and security advisories;
- screenshots from both themes and an automated accessibility report.

## Definition of done

Public alpha is ready when every P0 item is complete or explicitly listed as a
dated, owner-assigned known limitation; the clean-clone journey succeeds on a
new machine; a second operator restores the stack from backup; and an unfamiliar
agent completes the scoped demo loop using only published documentation and
public surfaces.
