# Public-alpha known limitations

**Snapshot:** July 11, 2026
**Owner:** Clotho maintainers
**Next review:** August 1, 2026, or before the first public-alpha tag

**Stage 22 disposition:** accepted by the product owner on July 11, 2026 for
public-alpha closure. These are scoped alpha constraints, not untracked work.

This register distinguishes honest alpha boundaries from release blockers. A
listed limitation does not override the fail-closed invariants in `AGENTS.md`
or the [threat model](threat-model.md).

## Accepted alpha boundaries

| Limitation                                              | User impact                                                                                                                                                                                              | Resolution owner / target                                                      |
| ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| Single-node Compose evaluation profile                  | No production, HA, autoscaling, zero-downtime upgrade, SLO, or Kubernetes claim. Do not use Clotho as the only copy of critical data.                                                                    | Maintainers; Stages 23–24 before a production claim.                           |
| Production tenant model is not yet accepted             | Current organizations/permissions are alpha control-plane behavior; hosted multi-tenant isolation, lifecycle, typed tenant context, RLS, quotas, and hostile two-organization evidence are not promised. | Identity/tenancy owner; Stage 23.                                              |
| `internal` repository visibility is reserved            | During alpha, `internal` is treated as non-public like `private`; it does not grant organization-wide discovery.                                                                                         | Identity/tenancy owner; Stage 23 policy decision.                              |
| No agent deletion/retention API                         | Revoked credentials stop authenticating, but agent identity, token metadata, and audit provenance remain until a retention policy and safe deletion workflow exist.                                      | Agent/audit owner; review during Stage 23 lifecycle work.                      |
| Repository deletion leaves bounded internal VCS residue | Clotho deletes metadata and the collaboration-provider project, but no atomic public VCS delete/reconciliation boundary exists yet. Long-running evaluation hosts may accumulate repository directories. | VCS/durability owner; before beta recovery acceptance.                         |
| Some provider checks require operator credentials       | Daytona, Box, ComputeSDK upstreams, managed Clerk, private/gated Hugging Face, and Tailscale live behavior cannot be represented as tested without credentials. Unconfigured paths fail closed.          | Provider owners; rerun for each release candidate with authorized credentials. |
| No official signed release artifacts yet                | Source can be built locally, but there are no tagged multi-platform CLI binaries, supported container bundle, checksums, SBOM, signatures, or provenance attestations.                                   | Release owner; first authorized alpha tag/registry run.                        |
| Compatibility begins with the first tag                 | `main` follows additive `/api/v1` rules, but no older tagged release is currently inside a supported compatibility or migration window.                                                                  | API/release owners; first alpha tag.                                           |
| Some REST collections and mutations retain alpha shapes | Repositories/activity are cursor-paged and Action starts are idempotent, but smaller metadata lists, other creates/imports/submits, conditional policy writes, and common cancellation handles are not compatibility-frozen. Clients must obey documented bounds and must not blindly retry mutations without an operation-specific key. | API owner; compatibility beta gate after the first alpha usage data. |
| CLI configuration remains automation-first              | The CLI has stable JSON/errors/exits and no hidden prompts, but named contexts, OS-keychain storage, generated completions/reference, common cancellation/timeouts, and broad disposable-fixture coverage are not part of the alpha promise. Tokens should be supplied by an external credential manager or environment. | CLI owner; beta usability gate, August 2026 review. |
| MCP schemas are alpha and operation-specific             | Tools are scope-filtered, audited, and REST-backed where public, but common schema metadata, capability classes, cancellation, and universal REST-equivalence are not frozen. Agents must use discovered schemas and stable REST error codes rather than cache assumptions. | Agent platform owner; beta MCP compatibility gate. |
| Journey-wide browser automation is incomplete            | Both themes, semantic contrast, dashboard/repository desktop and 320 px layouts are evidenced, but every public-alpha journey is not yet automated in Playwright. Alpha users may encounter route-specific loading/error/accessibility defects. | Web owner; weekly alpha triage, full suite before beta. |
| Operational bounds are route-specific                    | Major uploads, previews, imports, Actions, pages, retries, and idempotency records are bounded, but there is no single quota/backpressure framework or dependency-aware readiness contract. The supported profile is non-critical single-node evaluation only. | Operations owner; Stage 24 production profile. |
| First-tag evidence is necessarily pending                | The security and release workflows define advisory, secret, SAST, license/misconfiguration scans plus SBOM/checksum/keyless provenance outputs, but registry/container digests, a signed tag, and the workflow run can only exist for an authorized release tag. | Release owner; execute and attach on the first alpha tag. |
| Unfamiliar-agent acceptance is partially automated       | Clean-clone bootstrap and the live scoped MCP orient/change/checkpoint/restore path pass independently; one fully packaged black-box run that provisions authority without operator setup is not supplied because agents may not mint sibling credentials by design. | Agent/docs owner; rerun with a second operator before the first tag. |
| Provider connection metadata follows alpha org semantics | Agent discovery is sanitized and human credentials are validated, but human provider configuration is not yet projected through the Stage 23 typed tenant context. Do not treat the alpha provider screen as a cross-tenant production boundary. | Identity/provider owners; first Stage 23 authorization slice. |
| Key rotation and incident export are operator procedures  | Tokens revoke/expire, secrets are write-only, webhook replay is durable, and backups include the secrets key, but online secrets-master rotation and a packaged incident-safe audit export are not alpha APIs. | Security owner; beta security review. |
| Migration compatibility begins after the first tag        | Migrations are forward-only and the complete current database restores successfully; there is no previous public release fixture against which to promise rollback compatibility. | Durability owner; create the fixture from the first alpha tag and require it thereafter. |

## Stage 22 blocker status

There are no unregistered Stage 22 blockers. Every remaining public alpha
constraint is owned and dated above. Later work must not silently broaden any
of these promises.

## Support

Only the latest `main` pre-release receives best-effort fixes. See `SUPPORT.md`
for the tested operating envelope and `SECURITY.md` for private reporting.
