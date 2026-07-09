# Architecture Decision Records

Numbered, immutable records of significant architecture decisions. Format:
Context → Decision → Consequences. Supersede rather than edit.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-vcs-engine-jj-lib.md) | VCS engine built on jj-lib | Accepted |
| [0002](0002-storage-engine-xet-core.md) | Arachne storage engine built on xet-core | Accepted |
| [0003](0003-forgejo-integration-adopt.md) | Forgejo shell via shared git root + adopt | Accepted |
| [0004](0004-mcp-sdk-rmcp-streamable-http.md) | Agent gateway on rmcp over streamable HTTP | Accepted |
| [0005](0005-agent-identity-schema.md) | Agent identity schema in Postgres | Accepted |
| [0006](0006-merge-queue-integration.md) | Merge-queue serialized rebase + git ref import | Accepted |
| [0007](0007-edge-read-api-and-presence.md) | Edge read API; presence proxies the audit log | Accepted |
| [0008](0008-compute-cci-daytona.md) | Rust-native CCI; Daytona provider; git objects shipped to the sandbox | Accepted |
| [0009](0009-agent-write-tools-through-merge-queue.md) | Agent write tools route through VCS and merge-queue over MCP | Accepted |
| [0010](0010-clotho-cli-uses-rest-edge.md) | The clotho CLI uses the api-gateway REST edge | Accepted |
| [0011](0011-clotho-collaboration-facade.md) | Clotho owns the collaboration facade | Accepted |
| [0012](0012-actions-compute-control-plane.md) | Clotho owns Actions and the compute control plane | Accepted |
| [0013](0013-compute-provider-registry.md) | Capability-aware provider registry; ComputeSDK TS sidecar behind CCI | Accepted |
| [0014](0014-secrets-storage-and-resolution.md) | Secrets storage (encrypted Postgres) and compute credential resolution | Accepted |
