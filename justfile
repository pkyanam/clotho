# Clotho — cross-language task runner.
# Rust lives in the Cargo workspace; TS/JS in the pnpm workspace.

# Make cargo reachable from `just` even when the calling shell hasn't exported
# it: Homebrew's rustup is keg-only (/opt/homebrew/opt/rustup/bin) and rustup's
# shims live in ~/.cargo/bin. Non-existent entries are harmless on Linux/CI.
export PATH := env_var("HOME") + "/.cargo/bin:/opt/homebrew/opt/rustup/bin:" + env_var("PATH")

default:
    @just --list

# Read-only dependency and repository diagnostics. Pass `--json` for agents
# and `--stack` to require the running Docker/HTTP surfaces.
doctor *args:
    @./scripts/doctor.sh {{args}}

# Deterministic first command for an unfamiliar human or agent. This verifies
# prerequisites and prints actionable fixes; it does not mutate the worktree,
# install packages, contact providers, or remove Docker volumes.
bootstrap: doctor

# Install JS dependencies
setup:
    pnpm install

# Bring up the full dev stack (Postgres, MinIO, all Clotho services)
dev:
    docker compose -f docker-compose.dev.yml up --build

# Tear down the dev stack
dev-down:
    docker compose -f docker-compose.dev.yml down -v

# Optional ComputeSDK bridge sidecar (docs/adr/0013).
# Starts services/compute-sdk-bridge on :8091 via compose profile `compute-bridge`,
# and recreates clotho-compute so CLOTHO_COMPUTE_SDK_BRIDGE_URL is applied
# (compose env is only picked up on create/recreate, not by starting the bridge alone).
# Upstream keys: Clotho Settings → Compute (any ComputeSDK provider) or env.
# Image uses pnpm only. Does not tear down volumes.
dev-compute-bridge:
    docker compose -f docker-compose.dev.yml --profile compute-bridge up -d --build clotho-compute-sdk-bridge
    docker compose -f docker-compose.dev.yml up -d clotho-compute

# Host-run bridge with pnpm workspace (Node 20+).
dev-compute-bridge-host:
    pnpm --filter @clotho/compute-sdk-bridge start

# Optional StorageSDK provider bridge (:8092). The managed Arachne/MinIO path
# remains the zero-config default.
dev-storage-bridge:
    docker compose -f docker-compose.dev.yml --profile storage-bridge up -d --build clotho-storage-sdk-bridge

dev-storage-bridge-host:
    pnpm --filter @clotho/storage-sdk-bridge start

# Run all tests across both workspaces
test: test-rust test-js

test-rust:
    cargo test --workspace

test-js:
    pnpm turbo test

# Storage dedup integration tests against the dev stack's MinIO (`just dev`
# first). Override file size with CLOTHO_STORAGE_TEST_FILE_MB (default 256).
test-storage:
    CLOTHO_TEST_FAIL_ON_SKIP=1 CLOTHO_STORAGE_TEST_S3_ENDPOINT=http://localhost:9000 cargo test -p clotho-storage --test storage --release

# Stage 3 + 6 + 11 integration tests against the running dev stack (`just dev`
# first): repo creation through the Clotho API → real Forgejo project with
# working issues/PRs backed by a jj-managed git repo (tests/gateway.rs), the
# full browse/PR-diff/presence read surface (tests/stage6.rs), and the durable
# control plane (tests/stage11.rs). Missing live gates fail this recipe.
test-collab:
    #!/usr/bin/env bash
    set -euo pipefail
    forgejo_token="$(docker compose -f docker-compose.dev.yml exec -T clotho-api-gateway sh -c 'cat /run/clotho/forgejo-token')"
    CLOTHO_TEST_FAIL_ON_SKIP=1 \
      CLOTHO_COLLAB_TEST_GATEWAY_URL=http://localhost:8080 \
      CLOTHO_STAGE11_TEST_DATABASE_URL=postgres://clotho:clotho-dev@localhost:5432/clotho \
      CLOTHO_STAGE11_TEST_FORGEJO_TOKEN="$forgejo_token" \
      cargo test -p clotho-api-gateway --tests

# Stage 4 agent-interface integration test against the running dev stack
# (`just dev` first): a real MCP client authenticates as a scoped agent
# identity, checkpoints, breaks something, and restores — all over MCP.
test-agent:
    CLOTHO_TEST_FAIL_ON_SKIP=1 CLOTHO_AGENT_TEST_MCP_URL=http://localhost:8090 cargo test -p clotho-agent-gateway --test agent

# Stage 7/14 compute unit + integration tests. Live provider tests self-skip
# unless DAYTONA_API_KEY / BOX_API_KEY are set — loaded from .env if present.
test-compute:
    set -a; [ -f .env ] && . ./.env; set +a; cargo test -p clotho-compute -- --nocapture
    pnpm --filter @clotho/compute-sdk-bridge test

# Stage 7 end-to-end definition-of-done demo (`just dev` first). One command:
# two agent sessions push concurrent commits reconciled by the merge-queue, a
# large binary uploaded twice shows measured chunk dedup, a PR to review at
# :3100, and a push-triggered CI job on the real sandbox provider reporting
# status back. Reads .env for DAYTONA_API_KEY (CI leg self-skips without it).
demo:
    ./scripts/demo/run.sh

# Build everything
build: build-rust build-js

build-rust:
    cargo build --workspace

build-js:
    pnpm turbo build

# Lint & format checks
lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo fmt --all --check
    pnpm turbo lint

fmt:
    cargo fmt --all
    pnpm exec prettier --write "apps/**/*.{ts,tsx,css}" "packages/**/*.{ts,tsx,css}"
