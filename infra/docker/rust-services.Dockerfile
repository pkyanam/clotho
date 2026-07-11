# Multi-stage build for all Clotho Rust services.
# Each compose service selects its binary via the SERVICE build arg. BuildKit
# caches the Cargo registry and shared target directory across those builds,
# avoiding a full dependency rebuild for every service binary.

FROM rust:1.96-slim AS builder
ARG SERVICE
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY proto ./proto
# The gateways embed the public contract and capability inventory at compile time.
COPY openapi.yaml ./openapi.yaml
COPY docs/capabilities.json ./docs/capabilities.json
RUN --mount=type=cache,id=clotho-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=clotho-cargo-target,target=/build/target,sharing=locked \
    cargo build --release --bin "${SERVICE}" \
    && cp "/build/target/release/${SERVICE}" /tmp/clotho-service

FROM debian:trixie-slim AS runtime
ARG SERVICE
# uid 1000 matches Forgejo's git user, so the shared git-repos volume is
# writable from both sides (docker-compose.dev.yml, docs/adr/0003).
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 1000 --no-create-home clotho \
    && mkdir -p /var/lib/clotho/git-repos && chown -R clotho /var/lib/clotho
COPY --from=builder /tmp/clotho-service /usr/local/bin/service
USER clotho
WORKDIR /var/lib/clotho
ENTRYPOINT ["/usr/local/bin/service"]
