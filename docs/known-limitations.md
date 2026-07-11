# Public-alpha known limitations

**Snapshot:** July 11, 2026
**Owner:** Clotho maintainers
**Next review:** August 1, 2026, or before the first public-alpha tag

This register distinguishes honest alpha boundaries from release blockers. A
listed limitation does not override the fail-closed invariants in `AGENTS.md`,
the threat model, or `docs/release-readiness.md`.

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

## Open Stage 22 blockers (not accepted limitations)

The current release-gap matrix remains authoritative. In particular,
membership-safe user/organization directories, human provider-metadata
authorization, the complete route × identity/indirect-resource matrix,
complete backup/restore, migration/forward-repair evidence, supply-chain and
clean-clone/unfamiliar-agent journeys must be materially closed before a public
alpha is declared. This section is removed or converted to dated release
evidence as those controls land.

## Support

Only the latest `main` pre-release receives best-effort fixes. See `SUPPORT.md`
for the tested operating envelope and `SECURITY.md` for private reporting.
