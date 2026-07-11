# scripts/

One-off setup, migration, and seed scripts. Anything that runs more than once
or in production belongs in a service, not here.

- `postgres/`, `forgejo/` — one-shot dev-stack provisioning (run at first boot).
- `demo/run.sh` — the Stage 7 end-to-end definition-of-done demo (`just demo`):
  concurrent agent commits reconciled by the merge-queue, measured storage
  dedup, a PR to review at :3100, and a push-triggered CI job on the real
  Daytona sandbox reporting status back. Thin wrapper over the `clotho-demo`
  driver (`crates/clotho-demo`).
- `verify-api-contract.mjs` — deterministic Stage 22 OpenAPI/Axum/SDK
  structural verification. `pnpm test:contract -- --json` emits the complete
  machine-readable operation inventory for release diffs.
