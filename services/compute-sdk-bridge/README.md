# ComputeSDK bridge (optional CCI provider)

Small **TypeScript/Node HTTP sidecar** that sits *behind* the Clotho Compute
Interface (CCI). Clotho services never import ComputeSDK directly
(docs/adr/0013, docs/prd.md Stage 14).

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
  "files": [{ "path": "/tmp/x", "content_base64": "..." }],
  "credentials": {
    "e2b_api_key": "...",
    "modal_token_id": "...",
    "modal_token_secret": "...",
    "daytona_api_key": "..."
  }
}
```

Per-job `credentials` come from Clotho secrets (api-gateway → CCI
`provider_credentials`) and are preferred over process env for that job.
Response:

```json
{
  "exit_code": 0,
  "logs": "...",
  "provider": "e2b",
  "sandbox_id": "..."
}
```

## Run with Clotho (recommended)

Compose profile `compute-bridge` (does not start by default):

```bash
just dev-compute-bridge
# or:
docker compose -f docker-compose.dev.yml --profile compute-bridge up -d clotho-compute-sdk-bridge
```

`clotho-compute` defaults to
`CLOTHO_COMPUTE_SDK_BRIDGE_URL=http://clotho-compute-sdk-bridge:8091`.
The provider is **configured only when** the bridge is reachable **and**
upstream keys exist (process env on the bridge, or Clotho secrets injected
per job). URL alone never marks configured.

### Connect keys without host `.env`

1. Settings → Compute → Connect E2B (stores org secret `E2B_API_KEY`)
2. Or Settings → Secrets: `E2B_API_KEY`, `MODAL_TOKEN_ID`, `MODAL_TOKEN_SECRET`
3. Actions / `RunJob` resolves secrets and injects them into the bridge job body

Raw secret values are never returned to the browser.

## Configuration

| Env | Meaning |
|---|---|
| `PORT` | Listen port (default `8091`) |
| `CLOTHO_COMPUTE_SDK_STRATEGY` | `priority` (default) or `round-robin` |
| `CLOTHO_COMPUTE_SDK_FALLBACK` | `true`/`false` (default `true`) |
| `E2B_API_KEY` | Dev escape hatch for `@computesdk/e2b` |
| `MODAL_TOKEN_ID` / `MODAL_TOKEN_SECRET` | Dev escape hatch for Modal |
| `DAYTONA_API_KEY` | Optional ComputeSDK Daytona package (Clotho already has a direct Rust Daytona provider) |

Without any upstream credentials the bridge still serves `/health` with
`configured: false` so `clotho-compute` lists it honestly.

## Run (host Node)

```bash
cd services/compute-sdk-bridge
# optional: npm install computesdk @computesdk/e2b
PORT=8091 node src/server.mjs
# or: just dev-compute-bridge-host
```

## Relation to Box

Box (ascii.dev) is a **separate** direct CCI provider for persistent VMs
(https://docs.ascii.dev/llms.txt). It is not routed through ComputeSDK.
