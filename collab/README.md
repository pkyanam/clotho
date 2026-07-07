# collab/

The collaboration shell: Forgejo, run largely as-is (issues, PRs, orgs,
permissions), PostgreSQL-backed.

## Licensing boundary — read before touching

Forgejo ≥ v9.0 is **GPLv3**. Clotho itself is Apache-2.0. To keep the licensing
boundary clean:

- `collab/forgejo/` is a **git submodule pinned to a Forgejo release tag**
  (currently `v15.0.3`, the LTS line; shallow) — never vendored or merged
  into Clotho's own crates/packages. The dev stack runs the unmodified
  official container image at the same version.
- Any modifications to Forgejo live as patch files in `collab/patches/`,
  applied at build time. Modified Forgejo code we distribute stays GPLv3.
- Stage 3 shipped with **no Forgejo source changes at all** — Clotho talks to
  Forgejo purely over its REST API (adopt + repo/issue/PR endpoints) and by
  pointing it at git-compatible repos managed by `clotho-vcs` on a shared
  volume. See docs/adr/0003-forgejo-integration-adopt.md.

If you ever think you need to patch Forgejo source, stop: that is docs/prd.md
§8 open decision #2 and needs a deliberate human call first.
