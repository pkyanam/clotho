# Clotho

**A version control & collaboration platform for humans and AI agents.**

Clotho is the version control platform built for the world as it actually is
now: humans and agents, working together, on the same repo, at the same time.

- **Vision:** [docs/vision-spec.md](docs/vision-spec.md)
- **Prototype PRD:** [docs/prd.md](docs/prd.md)
- **Architecture decisions:** [docs/adr/](docs/adr/)

## Architecture (prototype)

```
                 ┌────────────┐
   human / agent │  apps/web   │
                 └─────┬──────┘
                       │ REST/GraphQL
                 ┌─────▼──────────────┐
                 │ clotho-api-gateway  │──────► Forgejo API (issues/PRs/org)
                 └─────┬───────────────┘
          ┌────────────┼─────────────────┐
          │             │                  │
    ┌─────▼─────┐ ┌─────▼──────┐  ┌───────▼────────┐
    │ clotho-vcs │ │clotho-storage│ │clotho-agent-gw  │◄── MCP clients (agents)
    │ (jj-lib)   │ │ (xet-core)   │ │ + merge-queue   │
    └─────┬──────┘ └──────┬───────┘ └────────┬────────┘
          │                │                    │
     git objects      MinIO / S3         Postgres (identity, audit)
```

## Repository layout

| Path | Purpose |
|---|---|
| `crates/` | Rust workspace: VCS engine, Arachne storage, merge queue, agent gateway, structured diff, API gateway, shared types |
| `apps/` | pnpm workspace: `web` (product app), `site` (marketing/teaser) |
| `packages/` | Shared TS libraries: `ui` (design system), `sdk-js`, `config` |
| `collab/` | Forgejo collaboration shell (submodule + patches, GPLv3 boundary) |
| `proto/` | Shared protobuf definitions |
| `infra/` | Dockerfiles, k8s/terraform (deferred) |
| `scripts/` | One-off setup/migration/seed scripts |

## Getting started

Prerequisites: Rust (stable), `protoc`, `just`, Node.js ≥ 20, `pnpm`, Docker.

```sh
just setup    # install JS deps
just dev      # bring up the full dev stack (docker compose)
just test     # run all tests across both workspaces
just build    # build everything
```

## License

Apache-2.0. See [LICENSE](LICENSE).

The `collab/forgejo` submodule is Forgejo (GPLv3) and is deliberately kept
outside Clotho's own codebase — see `collab/README.md`.
