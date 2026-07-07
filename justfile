# Clotho — cross-language task runner.
# Rust lives in the Cargo workspace; TS/JS in the pnpm workspace.

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

# Run all tests across both workspaces
test: test-rust test-js

test-rust:
    cargo test --workspace

test-js:
    pnpm turbo test

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
