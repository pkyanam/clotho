# ComputeSDK bridge (optional CCI provider)

Small **TypeScript/Node HTTP sidecar** that sits *behind* the Clotho Compute
Interface (CCI). Clotho services never import ComputeSDK directly
(docs/adr/0013, docs/prd.md Stage 12).

## Why a sidecar

ComputeSDK is TypeScript-native
([docs](https://docs.computesdk.com/llms.txt)): provider packages under
`@computesdk/*`, unified `compute.sandbox.create` / `runCommand` /
`filesystem.*` / `destroy`, and multi-provider
`providerStrategy: 'priority' | 'round-robin'` with `fallbackOnError`.

There is no Rust SDK. A minimal HTTP bridge keeps the Clotho backend all-Rust
while optionally unlocking broad provider coverage.

## HTTP contract

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/health` | `{ configured, message, providers[] }` — no secrets |
| `GET` | `/providers` | Upstream provider names known to this process |
| `POST` | `/jobs` | One-shot job: create sandbox → write files → run commands → destroy |

`POST /jobs` body:

```json
{
  "label": "repo@sha",
  "snapshot": "",
  "commands": ["echo hi"],
  "env": { "FOO": "bar" },
  "timeout_secs": 900,
  "files": [{ "path": "/tmp/x", "content_base64": "..." }]
}
```

Response:

```json
{
  "exit_code": 0,
  "logs": "...",
  "provider": "e2b",
  "sandbox_id": "..."
}
```

## Configuration

| Env | Meaning |
|---|---|
| `PORT` | Listen port (default `8091`) |
| `CLOTHO_COMPUTE_SDK_STRATEGY` | `priority` (default) or `round-robin` |
| `CLOTHO_COMPUTE_SDK_FALLBACK` | `true`/`false` (default `true`) |
| `E2B_API_KEY` | Enable `@computesdk/e2b` when the package is installed |
| `MODAL_TOKEN_ID` / `MODAL_TOKEN_SECRET` | Enable Modal |
| `DAYTONA_API_KEY` | Enable ComputeSDK's Daytona package (optional; Clotho already has a direct Rust Daytona provider) |

Without any upstream credentials the bridge still serves `/health` with
`configured: false` so `clotho-compute` can list it as unconfigured.

## Run (local)

```bash
cd services/compute-sdk-bridge
# optional: pnpm/npm install computesdk @computesdk/e2b
PORT=8091 node src/server.mjs
```

Point `clotho-compute` at it:

```bash
CLOTHO_COMPUTE_SDK_BRIDGE_URL=http://localhost:8091
```

Docker Compose does **not** start this by default (optional deploy unit).

## Relation to Box

Box (ascii.dev) is a **separate** CCI provider stub for persistent VMs
(https://docs.ascii.dev/llms.txt). It is not routed through ComputeSDK.
