# Stage 22 manual Action-idempotency evidence

**Date:** July 11, 2026

## Outcome and contract

`POST /api/v1/repos/{name}/actions/runs` accepts a bounded
`Idempotency-Key`. The first accepted request atomically commits the hashed key,
queued Action run, initial log, and exact `202` response. A same-intent retry
returns the original run without launching compute again and sets
`Idempotency-Replayed: true`; changed intent fails with the stable
`409 idempotency_conflict` envelope.

Keys are 1–128 ASCII characters using letters, digits, `.`, `_`, `:`, or `-`,
scoped to the immutable organization and authenticated principal, and retained
for 24 hours. Stored response bodies are bounded to 64 KiB. Expired records are
removed hourly in batches of at most 1,000. REST, OpenAPI, JavaScript SDK, CLI,
MCP, and web expose the same optional retry control.

## Migration and compatibility

Migration `1017_idempotency_records.sql` adds a common
`idempotency_records` table keyed by `(org_id, principal_id, key_hash)` with an
expiry index. It stores the operation and request fingerprint alongside the
resource id, status, and JSON response. Raw idempotency keys are never stored.
The run, initial log, and idempotency record share one Postgres transaction.

The request header and response header are additive. Existing clients that do
not provide a key still receive a normal queued run. `idempotency_conflict` is
an additive version-1 error code mapped to the existing CLI conflict exit class
`5`. Older gateways ignore no database state they own; migration downgrade and
previous-release forward-repair remain part of the wider Stage 22 operations
gate.

## Focused automated verification

The focused suites cover:

- valid, invalid, and oversized key parsing plus deterministic hashing;
- exact sequential replay, concurrent same-key collapse, changed-intent
  conflict, principal isolation, and one durable Action row;
- OpenAPI parameter/header/error shape and JavaScript SDK header forwarding;
- executable CLI header forwarding, one-value JSON stdout, and exit `5` for
  `idempotency_conflict`;
- MCP schema exposure plus live MCP↔REST replay and conflict equivalence.

Passed:

```sh
cargo test -p clotho-api-gateway persisted_action_idempotency_replays_and_conflicts
cargo test -p clotho-api-gateway keys_are_bounded_and_fingerprints_are_deterministic
cargo test -p clotho-cli --test action_idempotency
cargo test -p clotho-agent-gateway --test tool_schema
pnpm --filter @clotho/sdk-js test
pnpm test:contract
```

The gateway persistence tests passed against Postgres, including the concurrent
same-key race and cleanup of their unique organization/user fixtures. CLI
integration reported 2 passed tests, MCP schema integration reported 1 passed,
the SDK reported 30 passed, and contract verification reported 108 OpenAPI
operations, 108 Axum operations, 92 SDK calls, and 74 SDK interfaces.

## Runtime acceptance record

The development stack was rebuilt and restarted without removing volumes:

```sh
docker compose -f docker-compose.dev.yml up -d --build \
  clotho-api-gateway clotho-agent-gateway clotho-web
```

A unique repository and key were then exercised over live HTTP. The first and
second responses both named `run-34`; their replay headers were respectively
`false` and `true`. Reusing the key with a different actor returned HTTP `409`
and `idempotency_conflict`; an unsafe key returned HTTP `400` and
`invalid_request`. The Postgres catalog contained exactly one idempotency row
and one Action row for the returned resource.

After `docker compose -f docker-compose.dev.yml restart
clotho-api-gateway`, the same request again returned `run-34` with
`Idempotency-Replayed: true`, and the durable Action count remained one. A
second unique fixture exercised the compiled CLI: two `--json actions run`
calls returned `run-35`, changed intent exited `5`, stderr retained
`idempotency_conflict`, and stdout was empty on the error path. Both fixtures
and their test rows were removed after verification.

The production web build passed. A real browser loaded the repository Actions
page from the rebuilt stack and found one 36-character UUID-shaped hidden
`idempotency_key` on the manual-run form. Provider-unavailable behavior stayed
fail-closed: the run control was visibly disabled while Daytona was not
connected. The page rendered without visible layout failures in the reviewed
desktop viewport.

`just test-agent` passed against the rebuilt stack. Its scoped MCP client
started the Action twice with one key, received the same run id, and observed
the canonical `idempotency_conflict` error after changing the actor. The test
then revoked both fixture credentials and removed its exact `stage4-*`
repository through Clotho REST.

## Remaining risk

This closes one retryable mutation, not the Stage 22 idempotency gate. Repository
create, Hub import, release create, submit, collaboration mutations, token
minting, provider connect, and other retryable operations do not yet declare
the common contract. The broader authorization, recovery, packaging, project
hygiene, and public-alpha acceptance gates also remain open; Stage 23 is still
blocked until they are materially closed.
