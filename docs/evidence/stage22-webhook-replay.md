# Stage 22 webhook replay-defense evidence

**Date:** July 11, 2026
**Scope:** internal Forgejo push admission at
`POST /api/v1/webhooks/forgejo`

## Contract closed

- A configured `CLOTHO_WEBHOOK_SECRET` and constant-time HMAC-SHA256 over the
  exact request bytes are mandatory. Missing configuration, a missing/bad
  signature, or a missing control-plane database fails closed.
- Every delivery has a 1–128 byte visible-ASCII `X-Forgejo-Delivery` or
  `X-Gitea-Delivery` id. Conflicting aliases are rejected.
- Forgejo/Gitea event aliases are required, bounded, and must agree; only
  `push` reaches durable scheduling while other signed events are harmless.
- The repository name must resolve to exactly one Clotho control-plane row
  before any CI/provider side effect.
- Migration `1018_webhook_deliveries.sql` retains only SHA-256 hashes of the
  provider id and exact body, plus non-secret organization/repository/event/
  commit references. Neither the raw id nor payload is stored by this layer.
- A transaction reserves the delivery before scheduling. Concurrent identical
  requests produce exactly one `New` admission and one harmless `Replay`;
  changed bytes under the same id return stable `409 conflict`.
- Reservations expire after 24 hours. Opportunistic cleanup deletes at most
  1,000 expired rows per admitted delivery.

## Automated verification

```sh
cargo fmt --all --check
cargo clippy -p clotho-api-gateway --all-targets -- -D warnings
CLOTHO_WEBHOOK_TEST_DATABASE_URL=postgres://clotho:clotho-dev@localhost:5432/clotho \
  cargo test -p clotho-api-gateway webhooks::tests -- --nocapture
pnpm test:contract
```

Result: both focused webhook tests passed, including a 1,001-row expiry
fixture proving one cleanup call removed exactly the 1,000-row cap and the
next removed the remainder. Gateway all-target clippy and formatting passed.
The contract verifier reported 108 OpenAPI operations matching 108 Axum
operations, 92 SDK calls, and 74 SDK interfaces. Postgres reported migration
`1018:true`, and the test-owned webhook-delivery row count was zero after
cleanup.

The focused tests cover missing signing configuration, missing/invalid HMAC,
exact-body tampering, missing/conflicting event aliases, missing/oversized/
conflicting delivery headers, 40/64-character commit bounds, ambiguous
repository denial, atomic same-id/same-payload concurrency,
same-id/different-payload conflict, hashed
storage, and expired-row cleanup. The database fixture uses unique names and
removes its rows after success.

After the rebuilt Docker gateway applied migration 1018, a disposable
repository received a signed push through the live HTTP route. The first
delivery returned `202`; the API container restarted without removing volumes;
an exact replay then returned `200`, and changed bytes under the same delivery
id returned `409`. Deleting the test-owned repository through canonical REST
removed its reservation by foreign-key cascade; the post-check reported zero
owned repository and webhook rows. The stack was not torn down and no volume
was removed.

## Compatibility and limitations

This is an internal provider route, so no SDK, CLI, MCP, or web method is
introduced. Forgejo/Gitea signature and delivery header aliases remain
accepted. A process failure after durable reservation but before the detached
task begins is still observable as a missing Action and belongs to the broader
durable-operation/reconciliation gate; replay defense deliberately does not
claim a transactional outbox or exactly-once compute execution.

Live Daytona execution is credential-gated and was not required to prove
admission replay behavior; it must not be represented as exercised by this
evidence.
