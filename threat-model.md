# Public-alpha threat model

**Reviewed:** July 11, 2026
**Scope:** Clotho's public web, REST, SDK, CLI, MCP, Git/Hugging Face
compatibility, Compose deployment, provider boundaries, and recovery material.

This document models credible attacks against the public-alpha architecture. It
does not claim a completed independent security assessment or production
hardening. Stage 23 adds production identity and tenant isolation; Stage 24
adds the production control plane and deployment profiles.

## Assets and security objectives

- Repository history, files, model weights, datasets, releases, issues,
  comments, activity, Actions logs, and audit provenance retain confidentiality
  and integrity according to Clotho policy.
- Human, agent, provider, webhook, database, storage, and encryption
  credentials are never disclosed and cannot grant authority beyond their
  declared scope.
- A request cannot cross an organization/repository boundary through a direct
  name, an indirect run/job/release identifier, a cache key, storage prefix,
  provider call, webhook, or internal service hop.
- Destructive and externally visible operations are authenticated, authorized,
  auditable, bounded, and retry-safe.
- Operators can recover complete, mutually consistent state without exposing
  the secrets master key or silently restoring only one backing component.

Availability is best-effort during alpha, but memory, disk, queues, payloads,
logs, retries, and database work must still be bounded to prevent a trivial
denial of service.

## Trust boundaries

```text
browser / CLI / SDK / MCP client
              |
              v
     Clotho REST + MCP edges       untrusted Git/HF/provider inputs
              |                               |
      policy + durable metadata <-------------+
        /       |       \
  VCS/Git   Arachne/S3   compute/provider calls
        \       |       /
       backups, keys, audit, operator boundary
```

REST is the canonical public authority boundary. Internal gRPC and Forgejo are
not public contracts. Network location alone is never authorization. Webhook,
provider, repository, issue, imported metadata, Action log, model card, and
tool output are untrusted even when they came from an authenticated account.

## Threats and required controls

| Threat                                                                     | Required public-alpha controls                                                                                                                                                                                                    | Evidence / remaining work                                                                                                                                                                                                                                                       |
| -------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Human or agent token theft                                                 | Hash long-lived tokens at rest; prefix-identify type; expiry, revocation and least privilege; never log plaintext; stable 401/403; redact CI/test output.                                                                         | Human and agent hashing/revoke paths exist; CI masks/scans generated credentials. Rotation and complete permission-matrix evidence remain release-gate work.                                                                                                                    |
| Confused deputy across MCP, REST, providers, or Forgejo                    | Forward the originating principal; re-authorize at every boundary; caller cannot choose an internal scope header; Clotho selects repository/tool/provider intent; no broad service credential as user authority.                  | REST-backed MCP calls now forward and revalidate the original agent bearer against handler-owned repo/tool intent under required auth. Human directory/provider views and the complete route matrix remain Stage 22 work.                                                       |
| Cross-organization direct or indirect object access                        | Resolve organization and repository before downstream calls; filter before pagination/cache; conceal private existence consistently; include tenant in every durable job/storage/cache/audit key; test two hostile organizations. | Selected human permission tests exist. Typed tenant context, RLS defense in depth, and exhaustive indirect-ID inventory are Stage 23 acceptance.                                                                                                                                |
| SSRF through Hub imports, provider URLs, webhooks, or compatibility routes | Allow known provider schemes/hosts; reject credentials/userinfo, private/loopback/link-local targets unless an explicit private-network policy authorizes them; bound redirects/DNS changes; never return provider topology.      | Hugging Face source parsing is allowlisted and provider errors are normalized. Every configurable URL and redirect path still needs a single enforced egress policy audit.                                                                                                      |
| Archive/path traversal and unsafe file names                               | Normalize relative paths; reject absolute, `..`, empty, NUL and backslash variants before upload/extraction; refuse symlink/device escape; bound decompression and members.                                                       | Commit paths and release versions have focused tests. Backup/restore and every provider archive require the same verified extraction policy.                                                                                                                                    |
| Malicious model cards, issue text, logs, metadata, or tool output          | Treat content as data only; never translate prose into authorization or hidden instructions; escape in web; constrain parsers/previews; record provenance; require operator policy for execution.                                 | The invariant is documented in AGENTS/MCP. Browser/output encoding, prompt-injection regression fixtures, and artifact scanner coverage remain required evidence.                                                                                                               |
| Webhook forgery or replay                                                  | Verify HMAC over exact bytes in constant time; require secret; reject stale/replayed delivery IDs; make processing idempotent; keep provider payload untrusted.                                                                   | HMAC and database availability now fail closed; hashed 24-hour delivery reservations collapse concurrent/exact retries and reject changed payloads. A transactional Action outbox remains part of broader reconciliation work. |
| Sandbox escape or malicious workflow                                       | Do not execute on the control-plane host; use provider isolation and immutable inputs; require advertised capabilities; restrict credentials/network; bound time/output/resources; terminate and audit.                           | Capability checks and timeout policies exist. Live provider isolation is credential-gated and must be reported as skipped when not run; production workload-cell guarantees belong to Stage 24.                                                                                 |
| Secret disclosure at API, UI, logs, fixtures, backups, or errors           | Values are write-only; encrypt at rest; return metadata/last4 only; no plaintext snapshot; safe errors; backup key only through a protected operator file; bundle permissions 0700/0600.                                          | Secret APIs are metadata-only and bootstrap/CI logs are hardened. Key rotation and isolated full restore evidence remain release-gate work.                                                                                                                                     |
| Database/object-store/VCS split brain                                      | Quiesce writers for backup; capture all databases/roles, Git/VCS, object data, Forgejo state/token, and encryption key; checksum manifest; restore only into a new target; reconcile idempotently.                                | Action/import leases recover. Complete isolated backup/restore and outage reconciliation are Stage 22 blockers.                                                                                                                                                                 |
| Dependency or build-chain compromise                                       | Pin fast-moving dependencies and container bases; frozen lockfile installs; least-privilege CI; secret/SAST/license/dependency/container scans; SBOM, checksum, signature and provenance for releases.                            | Rust/web bases are digest referenced in current Dockerfiles. Full scanning, provider-image pinning, SBOM and signed release output remain release-gate work or explicitly dated artifact limitations.                                                                           |
| Denial of service                                                          | Authenticate costly paths; validate before side effects; cap bodies, pages, ranges, previews, logs, retries, concurrency, jobs and DB time; clean bounded fixtures; surface retryability.                                         | Several important paths are bounded and live tests now clean their prefixes. A machine-enforced resource-bound inventory is still required.                                                                                                                                     |

## Abuse cases that must fail closed

1. An invalid token supplied to an otherwise public route returns 401; it never
   silently downgrades to anonymous.
2. A foreign user or agent asking for a private repository or an indirect child
   receives the same safe response as a missing resource, before a Forgejo,
   VCS, object-store, cache, or provider call occurs.
3. Reusing an idempotency key with changed intent returns a stable conflict;
   retrying identical intent cannot launch a second durable operation.
4. A configured provider without a live capability probe remains unavailable.
5. A network-private repository cannot run on compute that lacks an attached
   private network capability.
6. A release with incomplete or mismatched immutable evidence cannot be served
   as ready or used for evaluation/inference.
7. Missing backup key material, checksum mismatch, unexpected archive members,
   or an existing restore target aborts before modifying durable state.

## Review and disclosure

Security-sensitive changes update this model and the route/tool permission
matrix. Reports use the private process in [SECURITY.md](SECURITY.md). Known
limitations are recorded in [known-limitations.md](known-limitations.md);
security invariants such as plaintext credential
logging, private-data exposure, webhook replay, and incomplete recovery cannot
be waived merely by relabeling them as limitations.
