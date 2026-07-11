# ADR-0021: Tailscale NetworkProvider — private reach, BYOC runners, private-cloud

- **Status:** Accepted
- **Date:** 2026-07-09
- **Deciders:** Clotho core
- **Related:** ADR-0019 (Provider Fabric), ADR-0013 (CCI), vision §4.2

## Context

Vision §4.2 calls for Tailscale (or WireGuard tailnet) as a first-class
network target: CI/agent sandboxes reach private services; optional
private-cloud mode keeps compute/storage inside the customer's network.
The Dream Roadmap elevates this to a product differentiator — GitHub can
*use* Tailscale in a workflow; Clotho should make **network identity part
of the org/repo** the way permissions are.

Tailscale already ships the primitives Clotho needs:

- OAuth clients with `auth_keys` scope
- Ephemeral, pre-approved, **tagged** nodes for CI
- ACL tags that map cleanly onto agent/runner identity

## Decision

### 1. `tailscale` as the first NetworkProvider

Under Provider Fabric (ADR-0019):

| Mode | What ships | Order |
|---|---|---|
| **Private reach** | Actions/sandbox jobs join the customer's tailnet as ephemeral tagged nodes | First |
| **BYOC runners** | `clotho-runner` on customer devices registers as a CCI compute provider | Second |
| **Private-cloud** | Storage + runners stay in-tailnet; Clotho control plane orchestrates only | Later |

Default NetworkProvider remains `public` (no mesh).

### 2. Connect Tailscale (org settings)

1. Org admin creates a Tailscale OAuth client (devices / `auth_keys` as
   required by Tailscale docs) and connects it in Clotho.
2. Client id + secret are stored via ADR-0014 secrets — never returned raw
   to the browser; UI shows configured/not + last health check.
3. Clotho suggests tag names and an ACL snippet, e.g.:
   - Org-wide: `tag:clotho-ci`
   - Repo-scoped (optional): `tag:clotho-{org}-{repo}`
4. Admin pastes the ACL into their tailnet policy (Clotho does not silently
   rewrite customer ACLs in v1 — generate + copy is enough).

REST sketch:

- `POST /api/v1/orgs/{org}/providers/network/tailscale/connect`
- `DELETE …/disconnect`
- `GET …/network` → metadata, suggested tags, ACL helper text
- Repo settings may set `network_policy: { mode, tags[] }`

### 3. Private reach for jobs

When a job (Actions run or sandbox) requires private network — or the repo
opts in — the compute path:

1. Uses the org's Tailscale OAuth client to mint an ephemeral auth path
   (same pattern as Tailscale's GitHub Action).
2. Joins the sandbox/runner to the tailnet with the configured tags.
3. Runs the job; on completion, logs out so the ephemeral node is removed.
4. Agent identity (Clotho) and node tags (Tailscale) are recorded on the
   Action run / sandbox session for audit.

Jobs that need private DBs/GPUs declare capability `private-net`. Fabric
scheduling fails closed if Tailscale is not configured.

### 4. BYOC: `clotho-runner`

Ship a thin runner agent (Rust binary, aligned with CLI packaging):

- Install on homelab / laptop / bare metal already on the customer's tailnet
  (or that will join via auth key).
- Registers with Clotho CCI as provider id like `byoc:{device}` advertising
  capabilities (cpu, optional gpu, persistent, private-net).
- Polls or listens for jobs the same way self-hosted Actions runners do —
  protocol details at implementation time, but **must** sit behind CCI so
  Actions never hard-code BYOC.
- Auth: device registration token minted by org admin (Clotho secret /
  one-time token); revoke disables the provider.

### 5. Private-cloud mode (deferred detail)

Control plane (api-gateway, web) may stay managed; object store and runners
resolve only to in-tailnet endpoints. Exact packaging (Helm, compose
profile) is a later design — this ADR only reserves the mode and forbids
claiming "private cloud" until storage + compute both honor the network
boundary.

### 6. Parity and honesty

- SDK/CLI/MCP: list/connect/disconnect network providers; show runner
  registration instructions; no secret values.
- `configured` for Tailscale means OAuth client present **and** a live
  probe (e.g. token mint or API whoami) succeeds — URL alone is not enough.
- Do not require Tailscale for `just demo` / default CI on Daytona public
  sandboxes.

## Consequences

- Network and agent permission stories can align: `tag:clotho-…` ↔ agent
  allowed_repos / job provenance.
- New operational surface: customers must understand Tailscale ACLs; UX
  must ship copy-paste helpers and clear failure messages.
- Compute providers that cannot join a tailnet (some hosted sandboxes)
  must advertise `private-net: false`; fabric won't select them for
  private-net jobs.
- Security: OAuth client compromise equals ability to mint nodes — treat
  like a cloud provider key; support rotate/disconnect.

## Implementation status (2026-07-11)

Shipped: zero-env persistent vault bootstrap for Compose; in-app OAuth client
connect/disconnect; live token probe; Provider Fabric metadata; generated grant
policy helper; repo `public|tailscale` mode and tags across REST/SDK/CLI/web;
Actions fail closed when private networking cannot be honored. Remaining:
ephemeral auth-key mint and provider-specific sandbox attachment, followed by
the `clotho-runner` BYOC registration protocol.
