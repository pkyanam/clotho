# Stage 22 complete-backup and restore evidence

**Date:** July 11, 2026

`just backup` captures separate clean Postgres dumps for the Clotho control
plane and collaboration metadata plus the MinIO, Forgejo, Git, VCS, Arachne,
and secrets-key volumes. The bundle contains SHA-256 checksums and refuses to
claim completeness when encrypted secret rows exist without a volume-managed
master key.

`just restore-drill <bundle>` verifies every checksum, restores both databases
with error-stop enabled, requires non-empty public schemas, extracts all six
durable volumes, verifies the secrets key when required, and removes all
disposable drill resources. It never replaces the running development volumes.

The July 11 drill used the live development stack. Its first attempt exposed
and corrected two real recovery defects: cluster-wide role restoration and an
incorrect Compose volume-name separator. The final drill reported:

```text
restore drill passed: Postgres plus six durable volumes
```
