# Contributing to Clotho

Clotho welcomes focused bug fixes, tests, documentation, and product proposals.
It is active public-alpha software, so contributors should read `AGENTS.md` and
`docs/release-readiness.md` before changing a public surface.

## Before opening a change

1. Search existing issues and ADRs for the behavior you plan to change.
2. Open an issue first for a new stable API, migration, provider boundary,
   security model, or architecture change.
3. Keep Forgejo under `collab/forgejo` unmodified. Clotho owns every public
   REST, SDK, CLI, MCP, and web behavior.
4. Never include credentials, private provider output, user data, or runtime
   volume contents in a fixture, log, screenshot, or commit.

## Development

Clone the submodule and run the deterministic checks:

```sh
git clone --recurse-submodules https://github.com/pkyanam/clotho.git
cd clotho
just bootstrap
just setup
just dev
just doctor --json --stack
```

Do not use `just dev-down` while preserving local data; it removes development
volumes. Service-backed tests use unique fixtures and clean only their declared
test prefixes.

Run the checks appropriate to the files you touched. The complete matrix is in
`AGENTS.md`; the baseline is:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck
pnpm lint
pnpm test
just test-collab
just test-agent
just test-storage
```

Credential-gated provider tests must be reported as skipped unless they were
actually executed.

## Contract and migration rules

- REST leads. Land OpenAPI, SDK, CLI, MCP, web, tests, and documentation with
  the REST behavior they expose.
- Changes within `/api/v1` are additive. Breaking changes require a versioned
  path, deprecation plan, migration note, and an ADR when architecture changes.
- Existing SQL migrations are immutable. Append a new forward migration; never
  edit or renumber one that may have run elsewhere.
- Secrets remain write-only after creation. New APIs may return metadata, never
  plaintext values.

## Pull requests

Use the repository pull-request template. Include exact verification commands,
live evidence, skipped gates, compatibility impact, migrations, and remaining
risk. A change is ready only when its relevant checklist is evidence-backed;
unit tests alone do not prove Docker, restart, browser, streaming, provider, or
recovery behavior.

By contributing, you agree that your contribution is licensed under the
repository's Apache-2.0 license.
