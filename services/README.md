# Optional services

Long-lived sidecars that are **not** part of the default `just dev` Rust stack.

| Service | Purpose |
|---|---|
| [`compute-sdk-bridge`](./compute-sdk-bridge/) | TypeScript HTTP bridge for ComputeSDK behind the CCI (docs/adr/0013). Optional compose profile `compute-bridge` / `just dev-compute-bridge`. Configured only when upstream keys exist (env or Clotho secrets per job). |
