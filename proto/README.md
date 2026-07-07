# proto/

Shared protobuf definitions — the single source of truth for inter-service
contracts. Consumed by:

- `crates/clotho-common` (via `tonic-build`/`prost`) for all Rust services
- `packages/sdk-js` (generated TS client) for the frontend

Conventions: one directory per service domain, versioned packages
(`clotho.<domain>.v1`).
