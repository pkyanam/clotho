# ADR-0013: Capability-aware provider registry; ComputeSDK as a TS sidecar behind CCI

- **Status:** Accepted
- **Date:** 2026-07-08
- **Deciders:** Clotho core

## Context

Stage 7/10 shipped a single-provider CCI: one `ComputeProvider` (`DaytonaProvider`
or `DisabledProvider`), one-shot `RunJob`, and gateway-local provider metadata
that hard-coded Daytona capabilities. Stage 12 (docs/prd.md §5) requires:

1. a **capability-aware provider registry** (id, configured state, features,
   regions, snapshots/templates, persistence, SSH, desktop, public URL, file
   APIs, terminal streaming, cost hints);
2. **Daytona** kept as the stable direct Rust provider;
3. an optional **ComputeSDK bridge** behind CCI (not a replacement of CCI);
4. design room for a later **Box** persistent-VM provider;
5. consistent exposure via **API + web + SDK**, without returning raw secrets.

PRD §11 open decision **#4** asked whether the ComputeSDK bridge should be a
small TypeScript sidecar, a separate long-lived service, or a constrained
worker process behind `clotho-compute`. Vision §4.1 still wants either
ComputeSDK or a compatible interface; ADR-0008 already chose Rust-native CCI
as the product boundary so Clotho never depends on a vendor SDK as the public
contract.

## Decision

### 1. CCI owns a multi-provider registry

`clotho-compute` holds a **`ProviderRegistry`**: zero or more named
`ComputeProvider` implementations, each advertising a structured
`ProviderDescriptor` (capabilities + non-secret config metadata).

- **gRPC** grows `ListProviders` / `GetProvider` alongside `RunJob`.
- `RunJob` accepts an optional `provider_id`. When empty, the registry routes
  by **required capabilities** (default: one-shot job execution) and the
  configured default provider id (`CLOTHO_COMPUTE_PROVIDER`).
- Callers (api-gateway Actions) **never hard-code Daytona**. They pass a
  provider id from Actions config / registry defaults, or omit it and let
  the registry resolve.

The trait gains descriptor access:

```text
ComputeProvider
  name() -> &str
  descriptor() -> ProviderDescriptor
  run_job(JobSpec) -> JobResult
```

`DisabledProvider` remains for "no credential" and unknown-id cases.

### 2. Capability model (honest metadata)

Capabilities are structured fields, not a free-form marketing list:

| Field | Meaning |
|---|---|
| `one_shot_jobs` | create → files → exec → tear down |
| `persistent_workspaces` | long-lived sandboxes / VMs |
| `snapshots` / `templates` | snapshot or template images |
| `regions` | advertised region codes (may be empty) |
| `ssh` / `desktop` / `public_url` | remote access surfaces |
| `file_api` / `terminal_streaming` | toolbox-style APIs |
| `cost_hints` | optional free-text / tier hints when known |

Configured state is **boolean + masked config** (e.g. "API key set", default
snapshot name). Secrets never leave process environment / bridge config.

### 3. ComputeSDK bridge = optional TypeScript HTTP sidecar

**Choice for PRD §11 #4:** a **small TypeScript sidecar service**
(`services/compute-sdk-bridge`), not an in-process worker and not a second
public product boundary.

ComputeSDK (https://docs.computesdk.com/llms.txt) is a TypeScript-native
unified sandbox API: install `@computesdk/<provider>` packages, use
`compute.sandbox.create` / `runCommand` / `filesystem.*` / `destroy`, and
optionally multi-provider `compute.setConfig` with `providerStrategy`
(`priority` | `round-robin`) and `fallbackOnError`. There is no Rust SDK.

- Speaks a minimal JSON HTTP API (`GET /health`, `GET /providers`,
  `POST /jobs`) so Rust needs no Node runtime.
- `clotho-compute` implements `ComputeSdkBridgeProvider` that proxies CCI
  jobs/descriptors to the sidecar when `CLOTHO_COMPUTE_SDK_BRIDGE_URL` is set.
- Without that URL (or when the sidecar reports unconfigured), the bridge
  provider is **registered as unconfigured** and never selected by default —
  the stack stays healthy, matching Daytona's no-key behavior.
- ComputeSDK packages (`@computesdk/e2b`, `@computesdk/modal`, …) and the
  core `computesdk` multi-provider router live only inside the sidecar;
  Clotho services depend solely on CCI.

Rationale:

- Keeps ADR-0008's all-Rust backend for the common path.
- Avoids embedding a Node runtime inside the compute container.
- Sidecar is an optional deploy unit (compose profile / env), not required
  for `just dev` or CI.
- A future "separate service" promotion is just giving the same binary its
  own deploy; the HTTP contract stays the same.

### 4. Box is a real adapter behind the same registry

A `BoxProvider` (Stage 14; was a Stage 12 stub) implements one-shot jobs
against the public Box API v1 (https://docs.ascii.dev/llms.txt, base
`https://ascii.dev/api/box/v1`, bearer `BOX_API_KEY` or per-job Clotho
secrets):

- one-shot: create → poll ready/idle → files → commands → delete;
- lifecycle hooks for later sessions: stop(archive) / resume / persistent create;
- capabilities: SSH, desktop/noVNC, public hosting, file API, snapshots.

Primary product model remains **persistent agent workspace**; one-shot CI is
supported. `configured` is true only when credentials can actually run jobs
(env key or gateway secret inject).

### 5. Public surface

- **REST:** `/api/v1/providers` (canonical) and `/api/v1/compute/providers`
  (alias, Stage 10) list the registry; secrets never returned.
- **Actions:** repo config `provider` is a registry id; empty means default.
  CI passes `provider_id` on `RunJob`.
- **Web/SDK:** multi-provider settings and Actions defaults show configured
  state and capabilities only.

## Consequences

- Adding a provider = new `ComputeProvider` impl + registry registration +
  env/docs; no frontend rewrite.
- Gateway can prefer live `ListProviders` from clotho-compute and fall back
  to env-derived descriptors if compute is unreachable (dev resilience).
- ComputeSDK coverage grows inside the sidecar without changing CCI.
- PRD §11 #4 is **resolved** (TS sidecar). #5 (durable Actions store) remains
  open; Stage 10/11 already use Postgres for runs but product history design
  is separate.
- Supersedes the Stage 10 "hard-coded Daytona metadata in the gateway" detail
  of ADR-0012 without changing Actions ownership or Forgejo status sync.
