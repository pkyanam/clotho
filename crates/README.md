# crates/

Rust workspace members — all of Clotho's backend services.

| Crate | Purpose |
|---|---|
| `clotho-common` | Shared protobuf-generated types, error types, tracing setup. Every other crate depends on it. |
| `clotho-vcs` | The VCS engine. Wraps `jj-lib`; gRPC service: init, commit, checkpoint, restore, op-log query. Git-compatible via gitoxide backend. |
| `clotho-storage` | The Arachne storage engine. Wraps `xet-core`; chunk/xorb upload + download against S3-compatible storage. |
| `clotho-merge-queue` | Multi-workspace reconciliation — serializes/rebases concurrent agent commits. The hardest, most novel piece. |
| `clotho-agent-gateway` | MCP server; agent identity & permission enforcement, per-call audit logging. |
| `clotho-diff` | Tree-sitter based structured diff engine; feeds both the human PR view and the agent-facing diff API. |
| `clotho-api-gateway` | Edge REST aggregation service (Axum); proxies to Forgejo's API. |

Internal services speak gRPC (protobuf definitions in `/proto`); the edge is
REST/JSON. No crate shells out to the `jj` or `git` CLI at runtime.
