# Optional services

Long-lived sidecars that are **not** part of the default `just dev` Rust stack.

| Service | Purpose |
|---|---|
| [`compute-sdk-bridge`](./compute-sdk-bridge/) | TypeScript HTTP bridge for ComputeSDK behind the CCI (docs/adr/0013). Disabled unless `CLOTHO_COMPUTE_SDK_BRIDGE_URL` points at it. |
