# ComputeSDK bridge (optional CCI provider)

Small **TypeScript/Node HTTP sidecar** that sits *behind* the Clotho Compute
Interface (CCI). Clotho services never import ComputeSDK directly
(docs/adr/0013).

## Why a sidecar

ComputeSDK is TypeScript-native
([docs](https://docs.computesdk.com/llms.txt)): provider packages under
`@computesdk/*`, unified `compute.sandbox.create` / `runCommand` /
`filesystem.*` / `destroy`, and multi-provider
`providerStrategy: 'priority' | 'round-robin'` with `fallbackOnError`.

There is no Rust SDK. A minimal HTTP bridge keeps the Clotho backend all-Rust
while unlocking **every** ComputeSDK provider
([providers](https://docs.computesdk.com/providers.md)).

## Supported upstreams

Catalog lives in `src/providers.mjs` and is exposed as `GET /catalog`. Includes
AgentCore, Agentuity, Archil, Beam, Blaxel, Cloudflare, CodeSandbox, Daytona,
Declaw, E2B, Freestyle, HopX, Kubernetes, Leap0, Modal, Namespace, Runloop,
Tensorlake, Upstash, and Vercel.

Only providers with credentials **and** an installed `@computesdk/*` package
are activated. Missing packages are skipped (dynamic `import`).

## Package manager

**pnpm only** (monorepo workspace member under `services/*`). Do not use npm
or yarn.

```bash
# from repo root
pnpm install
pnpm --filter @clotho/compute-sdk-bridge test
pnpm --filter @clotho/compute-sdk-bridge start
```

Install additional upstream packages (optionalDependencies are already listed):

```bash
pnpm --filter @clotho/compute-sdk-bridge add @computesdk/e2b @computesdk/vercel
```

## HTTP contract

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/health` | `{ configured, message, providers[], catalog[] }` |
| `GET` | `/catalog` | Full upstream catalog + required secret names |
| `GET` | `/providers` | Configured upstream names for this process |
| `POST` | `/jobs` | One-shot job |

`POST /jobs` body:

```json
{
  "label": "repo@sha",
  "commands": ["echo hi"],
  "env": {},
  "timeout_secs": 900,
  "files": [{ "path": "/tmp/x", "content_base64": "..." }],
  "credentials": {
    "E2B_API_KEY": "...",
    "VERCEL_TOKEN": "...",
    "VERCEL_TEAM_ID": "...",
    "VERCEL_PROJECT_ID": "..."
  },
  "upstream_provider": "e2b"
}
```

Credential keys are UPPER_SNAKE env names from the ComputeSDK installation
guide. Clotho gateway injects them from org/repo secrets.

## Run with Clotho

```bash
just dev-compute-bridge
# or: docker compose -f docker-compose.dev.yml --profile compute-bridge up -d clotho-compute-sdk-bridge
```

Settings → Compute → pick any ComputeSDK upstream and paste required secrets.
Values are never returned to the browser.

## Configuration

| Env | Meaning |
|---|---|
| `PORT` | Listen port (default `8091`) |
| `CLOTHO_COMPUTE_SDK_STRATEGY` | `priority` (default) or `round-robin` |
| `CLOTHO_COMPUTE_SDK_FALLBACK` | `true`/`false` (default `true`) |
| *(provider keys)* | See [installation](https://docs.computesdk.com/getting-started/installation.md) |

Without credentials the bridge serves `/health` with `configured: false`.

## Relation to Box / Daytona

- **Box** (ascii.dev) is a separate direct CCI provider.
- **Daytona** has a direct Rust CCI provider; `@computesdk/daytona` is also
  available on this bridge when `DAYTONA_API_KEY` is set.
