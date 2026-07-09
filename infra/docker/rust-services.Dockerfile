# Multi-stage build for all Clotho Rust services.
# Each compose service selects its binary via the SERVICE build arg, so the
# whole workspace compiles once and layers are shared across services.

FROM rust:1.96-slim AS builder
ARG SERVICE
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY proto ./proto
# Stage 15: api-gateway embeds docs/openapi.yaml via include_str!.
COPY docs ./docs
RUN cargo build --release --bin "${SERVICE}"

FROM debian:trixie-slim AS runtime
ARG SERVICE
# uid 1000 matches Forgejo's git user, so the shared git-repos volume is
# writable from both sides (docker-compose.dev.yml, docs/adr/0003).
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 1000 --no-create-home clotho \
    && mkdir -p /var/lib/clotho/git-repos && chown -R clotho /var/lib/clotho
COPY --from=builder "/build/target/release/${SERVICE}" /usr/local/bin/service
USER clotho
WORKDIR /var/lib/clotho
ENTRYPOINT ["/usr/local/bin/service"]
