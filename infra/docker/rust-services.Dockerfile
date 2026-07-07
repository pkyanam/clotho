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
RUN cargo build --release --bin "${SERVICE}"

FROM debian:trixie-slim AS runtime
ARG SERVICE
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home clotho
COPY --from=builder "/build/target/release/${SERVICE}" /usr/local/bin/service
USER clotho
ENTRYPOINT ["/usr/local/bin/service"]
