# collab/

The collaboration shell: Forgejo, run largely as-is (issues, PRs, orgs,
permissions), PostgreSQL-backed.

## Licensing boundary — read before touching

Forgejo ≥ v9.0 is **GPLv3**. Clotho itself is Apache-2.0. To keep the licensing
boundary clean:

- `collab/forgejo/` will be a **git submodule pinned to a Forgejo release**
  (added in Stage 3) — never vendored or merged into Clotho's own
  crates/packages.
- Any modifications to Forgejo live as patch files in `collab/patches/`,
  applied at build time. Modified Forgejo code we distribute stays GPLv3.
- The prototype plan (docs/prd.md §5, Stage 3) assumes **no Forgejo source
  changes at all** — Clotho talks to Forgejo purely over its API and by pointing
  it at git-compatible repos managed by `clotho-vcs`.

See docs/prd.md §8 for the open decision on deeper integration.
