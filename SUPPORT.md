# Support policy

Clotho is public-alpha software intended for evaluation and non-critical
self-hosting. It is not yet supported for production, high availability, or as
the sole copy of important data.

## Getting help

- Use GitHub Discussions when enabled for setup and design questions.
- Use a GitHub issue for reproducible bugs and documentation gaps.
- Use the private process in `SECURITY.md` for vulnerabilities.

Include the source commit, operating system, architecture, Docker and Compose
versions, `just doctor --json --stack` output, the failing public command, and a
redacted error envelope or log excerpt. Never attach `.env`, token files,
database dumps, private repository data, or provider credentials.

## Public-alpha operating envelope

The Compose profile is evaluated with at least 4 CPU cores, 8 GiB RAM, 40 GiB
free disk, Docker Engine 27 or newer, and Docker Compose 2.30 or newer. The July
11, 2026 acceptance host used Docker 29.6.1 and Compose 5.2.0. Provider-backed
Actions and imports need additional memory, disk, network access, and the
provider's own supported environment.

Back up persistent data before upgrading. Existing migrations are forward-only
and immutable; use a complete restore rather than a partial database rollback.
`just dev-down` is a clean uninstall of development containers **and volumes**,
so do not run it when data must survive.

Best-effort community support covers the latest `main` pre-release only. No
response-time, availability, compatibility-window, or data-recovery SLA is
offered during alpha.
