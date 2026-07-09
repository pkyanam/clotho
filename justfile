# Clotho — cross-language task runner.
# Rust lives in the Cargo workspace; TS/JS in the pnpm workspace.

# Make cargo reachable from `just` even when the calling shell hasn't exported
# it: Homebrew's rustup is keg-only (/opt/homebrew/opt/rustup/bin) and rustup's
# shims live in ~/.cargo/bin. Non-existent entries are harmless on Linux/CI.
export PATH := env_var("HOME") + "/.cargo/bin:/opt/homebrew/opt/rustup/bin:" + env_var("PATH")

default:
    @just --list

# Install JS dependencies
setup:
    pnpm install

# Bring up the full dev stack (Postgres, MinIO, all Clotho services)
dev:
    docker compose -f docker-compose.dev.yml up --build

# Tear down the dev stack
dev-down:
    docker compose -f docker-compose.dev.yml down -v

# Optional ComputeSDK bridge sidecar (Stage 14, docs/adr/0013).
# Starts services/compute-sdk-bridge on :8091 via compose profile `compute-bridge`.
# Point clotho-compute at it with:
#   CLOTHO_COMPUTE_SDK_BRIDGE_URL=http://clotho-compute-sdk-bridge:8091
# (in-cluster) or http://localhost:8091 (host-run compute). Upstream keys:
# Clotho Settings → Compute (E2B/Modal secrets) or env on this service.
# Does not tear down volumes. Safe to run alongside `just dev`.
dev-compute-bridge:
    docker compose -f docker-compose.dev.yml --profile compute-bridge up -d --build clotho-compute-sdk-bridge

# Host-run bridge without Docker (Node 20+). Upstream packages optional.
dev-compute-bridge-host:
    cd services/compute-sdk-bridge && PORT=8091 node src/server.mjs

# Run all tests across both workspaces
test: test-rust test-js

test-rust:
    cargo test --workspace

test-js:
    pnpm turbo test

# Storage dedup integration tests against the dev stack's MinIO (`just dev`
# first). Override file size with CLOTHO_STORAGE_TEST_FILE_MB (default 256).
test-storage:
    CLOTHO_STORAGE_TEST_S3_ENDPOINT=http://localhost:9000 cargo test -p clotho-storage --test storage --release

# Stage 3 + 6 integration tests against the running dev stack (`just dev`
# first): repo creation through the Clotho API → real Forgejo project with
# working issues/PRs backed by a jj-managed git repo (tests/gateway.rs), and
# the full browse/PR-diff/presence read surface (tests/stage6.rs).
test-collab:
    CLOTHO_COLLAB_TEST_GATEWAY_URL=http://localhost:8080 cargo test -p clotho-api-gateway --tests

# Stage 4 agent-interface integration test against the running dev stack
# (`just dev` first): a real MCP client authenticates as a scoped agent
# identity, checkpoints, breaks something, and restores — all over MCP.
test-agent:
    CLOTHO_AGENT_TEST_MCP_URL=http://localhost:8090 cargo test -p clotho-agent-gateway --test agent

# Stage 7/14 compute unit + integration tests. Live provider tests self-skip
# unless DAYTONA_API_KEY / BOX_API_KEY are set — loaded from .env if present.
test-compute:
    set -a; [ -f .env ] && . ./.env; set +a; cargo test -p clotho-compute -- --nocapture
    cd services/compute-sdk-bridge && node --test test/server.test.mjs

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
