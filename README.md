<div align="center">
  <img src="./logo-placeholder.svg" alt="Clotho" width="112" height="112" />

  <h1>Clotho</h1>

  <p><strong>The source of truth for software, models, datasets, and the humans and agents that build them.</strong></p>

  <p>
    Open source · Self-hostable · Git compatible · Agent native
  </p>

  <p>
    <a href="./docs/vision-spec.md">Vision</a>
    · <a href="./docs/prd.md">Product roadmap</a>
    · <a href="./docs/release-readiness.md">Release readiness</a>
    · <a href="./docs/frontier-roadmap.md">What comes next</a>
    · <a href="./docs/api.md">API</a>
    · <a href="./docs/mcp.md">MCP</a>
  </p>
</div>

<!--
PUBLIC RELEASE SCREENSHOT

Add the final light/dark product screenshot here before announcing the release:

<p align="center">
  <img src="./docs/assets/clotho-dashboard.png" alt="Clotho dashboard" width="1200" />
</p>
-->

---

Clotho is a modular version-control and collaboration platform designed for a
world where humans and AI agents work concurrently. It combines Git-compatible
history, Jujutsu-native operations, content-defined artifact storage, durable
automation, model and dataset management, and scoped agent tools behind one
Clotho-owned control plane.

Clotho is not a skin over Forgejo and it is not an ML registry bolted onto Git.
Forgejo is an internal collaboration provider; compute, storage, networking,
identity, and Hub integrations are replaceable modules. The public product
surface is Clotho: web, REST, SDK, CLI, Git/Hugging Face compatibility, and MCP.

> **Release status:** active pre-release software. The core platform is working
> end to end, but the public API stability, security review, accessibility,
> packaging, migration, and recovery gates in
> [release readiness](./docs/release-readiness.md) must close before a stable
> production claim.

## Why Clotho

### One repository, every artifact

Code, model weights, datasets, evaluation evidence, cards, releases, and
automation provenance share one commit graph. Large artifacts stream through
**Arachne**, Clotho's content-defined storage engine, while normal Git clients
continue to see real Git objects and standard LFS-compatible pointers.

### Agents are identities, not API keys with names

Agents receive scoped, revocable credentials; checkpoint and restore through
the repository operation log; consume structured diffs; submit through a
merge queue; and leave an auditable record of every action. Human and
agent interfaces resolve to the same product contract.

### Bring your own infrastructure

Clotho has stable provider boundaries for compute, object storage, networking,
and authentication. Daytona, ComputeSDK, StorageSDK, S3-compatible stores, and
Tailscale fit behind Clotho-owned capability contracts instead of leaking
vendor configuration into every repository.

### A model and dataset platform that stays portable

Clotho imports pinned Hugging Face snapshots, classifies semantic artifacts,
fails closed on unsafe scans, produces immutable verified releases, and serves
those releases through Clotho-native streaming and standard Hugging Face read
routes. Workloads can run against exact release digests without silently
falling back to a mutable hosted revision.

## What works today

- **Git-compatible VCS:** jj-lib-backed operations that write real Git objects,
  with checkpoints, operation history, structured diffs, and conflict-aware
  submission.
- **Arachne storage:** content-defined deduplication over S3-compatible storage,
  transparent large-file pointers, byte-range reads, and optional StorageSDK
  adapters.
- **Typed repositories:** `code`, `model`, and `dataset` policies with semantic
  manifests, cards, bounded previews, evaluations, and artifact readiness.
- **Hub migration and compatibility:** durable Hugging Face imports plus
  model/dataset discovery, refs, commits, trees, `HEAD`, and `resolve` reads.
- **Immutable releases:** commit- and manifest-bound versions with tamper
  verification and reproducible evaluation, inference, and benchmark Actions.
- **Actions and GPU policy:** capability-aware compute through CCI, Daytona,
  and an optional ComputeSDK bridge, including repository-level GPU intent.
- **Provider fabric:** Clotho-managed compute, storage, network, Hub, and auth
  connections with encrypted secrets and honest configured-state reporting.
- **Private networking:** Tailscale connection and repository network intent,
  designed to fail closed when private reach is unavailable.
- **First-class agents:** MCP tools for VCS, issues, pull requests, Actions,
  repositories, providers, activity, and bounded file reads.
- **Clotho control plane:** organizations, permissions, tokens, secrets,
  notifications, audit activity, merge policy, and a native web console.

## Architecture

```text
 humans                         agents
 browser · CLI · SDK            MCP clients
        │                           │
        └───────────┬───────────────┘
                    ▼
          ┌─────────────────────┐
          │ Clotho control plane│
          │ REST · auth · policy│
          └──────┬───────┬──────┘
                 │       │
       ┌─────────▼──┐  ┌─▼──────────────┐
       │ VCS + diff │  │ Actions + agents│
       │ jj · Git   │  │ queue · CCI     │
       └──────┬─────┘  └───────┬────────┘
              │                │
       ┌──────▼──────┐  ┌──────▼────────────────┐
       │ Arachne     │  │ modular providers      │
       │ chunks · S3 │  │ compute · network · Hub│
       └─────────────┘  └───────────────────────┘

 Internal implementation providers: Forgejo · Postgres · MinIO
```

| Component | Responsibility |
|---|---|
| `apps/web` | Clotho's human control plane |
| `clotho-api-gateway` | Canonical REST contract, auth, policy, and composition |
| `clotho-agent-gateway` | MCP transport, scoped agent identity, and audit |
| `clotho-vcs` | jj-lib operation graph and Git-compatible objects |
| `clotho-diff` | Tree-sitter structured diffs |
| `clotho-storage` | Arachne chunk storage and reconstruction |
| `clotho-merge-queue` | Serialized conflict-aware integration |
| `clotho-compute` | Capability-based compute interface (CCI) |
| Forgejo | Internal Git/collaboration compatibility provider |

Architectural decisions are recorded in [`docs/adr`](./docs/adr).

## Quick start

### Requirements

- Docker Desktop or another Compose-compatible Docker runtime
- Rust stable and `protoc`
- Node.js 20+, `pnpm`, and `just`

### Run Clotho locally

```sh
git clone https://github.com/pkyanam/clotho.git
cd clotho
just setup
just dev
```

Open [http://localhost:3100](http://localhost:3100).

The standard local stack does **not** require a `.env` file. Provider
credentials can be connected inside Clotho; environment variables remain an
automation and deployment escape hatch.

| Surface | Local address |
|---|---|
| Web | `http://localhost:3100` |
| REST / OpenAPI | `http://localhost:8080` · `/openapi.yaml` |
| MCP | `http://localhost:8090/mcp` |
| Forgejo debug provider | `http://localhost:13000` |
| MinIO | `http://localhost:9000` |

### Verify the workspace

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck
pnpm lint
pnpm test
```

Stack-dependent checks:

```sh
just test-collab
just test-agent
just test-storage
```

Do not run `just dev-down` casually: it removes local Docker volumes.

## Use Clotho

The REST API is the canonical public contract. The web app and SDK use it;
the CLI is a thin human client; MCP exposes a deliberately scoped agent
surface. New product capabilities are not considered mature until these
surfaces agree.

```sh
export CLOTHO_API_URL=http://localhost:8080

clotho repo init my-model --kind model
clotho repo import-hf my-model openai-community/gpt2@main
clotho repo imports my-model
clotho repo artifacts my-model
clotho repo release my-model v1.0.0
clotho actions run my-model --workflow evaluate --release v1.0.0
```

Developer guides:

- [REST and JavaScript SDK](./docs/api.md)
- [CLI](./docs/cli.md)
- [MCP agent gateway](./docs/mcp.md)
- [OpenAPI contract](./docs/openapi.yaml)

## Repository layout

| Path | Purpose |
|---|---|
| [`apps/web`](./apps/web) | Main product console |
| [`apps/site`](./apps/site) | Public marketing site |
| [`crates`](./crates) | Rust services and CLI |
| [`packages/sdk-js`](./packages/sdk-js) | Typed JavaScript client |
| [`packages/ui`](./packages/ui) | Shared design tokens and components |
| [`services`](./services) | Optional provider bridges |
| [`proto`](./proto) | Internal protobuf contracts |
| [`collab`](./collab) | Isolated Forgejo boundary |
| [`infra`](./infra) | Deployment assets |
| [`docs`](./docs) | Vision, PRD, plans, and ADRs |

## Roadmap

The immediate priority is a trustworthy public alpha: contract hardening,
security boundaries, backup and restore, migrations, accessibility, release
packaging, and a complete agent handoff path. The longer horizon includes
versioned agent handoff capsules, an evidence graph, GPU/data-local compute
bindings, lazy virtual repositories, and a protocol mesh that can project one
Clotho release as Git, Hugging Face, OCI, and artifact-storage interfaces.

Read the prioritized [frontier roadmap](./docs/frontier-roadmap.md) and the
stage-by-stage [PRD](./docs/prd.md).

## Open source and provider boundaries

Clotho's own code is licensed under [Apache-2.0](./LICENSE). Forgejo is GPLv3
and remains an unmodified, separately distributed internal provider behind a
runtime/API boundary. See [the collaboration boundary](./collab/README.md) and
[ADR-0003](./docs/adr/0003-forgejo-integration-adopt.md).

## The name

Clotho is the Fate who spins the thread. The platform is built around the same
idea: many human and agent hands continuously spinning one coherent history
forward.
