# ADR-0003: Wire Forgejo to clotho-vcs repos via a shared git root + adopt API

- **Status:** Accepted
- **Date:** 2026-07-07
- **Deciders:** Clotho core

## Context

Stage 3 (docs/prd.md §5) puts Forgejo in as the collaboration shell: issues,
PRs, org/permissions chrome over the repositories `clotho-vcs` manages. Two
hard constraints shape the wiring:

- **Licensing** (collab/README.md): Forgejo ≥ v9 is GPLv3, Clotho is
  Apache-2.0. Forgejo runs as a separate process from an unmodified official
  image, pinned to the same release as the `collab/forgejo` submodule
  (v15.0.3 — the current LTS line, supported to July 2027; the v14 line went
  EOL 2026-04-30). Clotho talks to it only over its REST API and via git
  repos on disk.
- **No git CLI in Clotho** (docs/prd.md §6): no Clotho service may shell out
  to `jj` or `git` at runtime.

How does Forgejo *see* the repos? Options considered:

- **Push-mirror** (clotho-vcs pushes to Forgejo over the git protocol) —
  rejected: gitoxide cannot push yet, so this would require shelling out to
  `git`, violating §6. It would also store every repo twice.
- **Symlinks from Forgejo's repo root into the jj store**
  (`<repo>/store/git`) — rejected: fragile across container mount
  namespaces, and Forgejo's adopt scan doesn't reliably traverse them.
- **Shared git root + Forgejo's adopt API** — chosen, see below.

## Decision

`clotho-vcs` becomes the single owner of the bare git repositories, created
where Forgejo expects to find them:

- The engine supports jj's **external git backend**: with
  `CLOTHO_VCS_GIT_REPOS_DIR` set, `init_repo` creates the backing bare git
  repo at `<git_root>/<name>.git` (jj metadata stays in
  `CLOTHO_VCS_DATA_DIR`). In the dev stack a shared `git-data` volume is
  mounted at that path in clotho-vcs and at `[repository].ROOT`
  (`/data/git/repositories`) in Forgejo, laid out as `<owner>/<repo>.git`
  with the `clotho` admin user as owner. Both containers run uid 1000 so
  adoption (which writes hooks into the repo dir) works.
- After every engine operation that moves history (`commit`, `restore_to`),
  the engine mirrors its `main` bookmark to `refs/heads/main` and keeps HEAD
  a symref to it (`mirror_main_ref`, via gix — no git CLI), so plain-git
  consumers see an ordinary branch and default branch.
- **Repo creation is one edge call**: `POST /api/v1/repos` on
  `clotho-api-gateway` (Axum, the first real REST endpoint) calls clotho-vcs
  `InitRepo`, seeds `main` with an empty initial commit, then calls Forgejo's
  admin **adopt** endpoint (`POST /api/v1/admin/unadopted/{owner}/{repo}`,
  enabled via `ALLOW_ADOPTION_OF_UNADOPTED_REPOSITORIES`) so Forgejo
  registers the already-on-disk repo as a full project. The gateway
  authenticates with a token minted at first boot by a one-shot provisioning
  container (scripts/forgejo/provision.sh) and shared via a volume.

The initial commit is seeded *before* adoption because Forgejo records
emptiness/default-branch at adoption time; adopting a non-empty repo makes
subsequent engine-written commits on `main` render live (verified: commits
written through the vcs gRPC API after adoption appear in Forgejo's commit
list and file views with no sync step).

## Consequences

- Git objects exist exactly once, owned by clotho-vcs; Forgejo is a
  replaceable read-side chrome, consistent with the vision spec's decoupled
  layers. No Forgejo source is modified (PRD §8 open decision #2 stays open).
- Writes through Forgejo (merging a PR in its UI, pushing over its HTTP
  endpoint, branch creation) update git refs behind jj's back; jj neither
  imports nor depends on git refs, so nothing corrupts, but such commits are
  invisible to the jj op log until an import exists. Acceptable for the
  prototype: Stage 5's merge-queue owns merges, and agent/human writes go
  through Clotho's APIs. Revisit with `jj git import` if Forgejo-side writes
  ever need to flow back.
- Forgejo caches branch lists in its DB (synced on push); new branches
  created behind its back may lag in the branches UI. Commits to `main` (the
  engine's only exported ref) render live, which is what Stage 3 needs.
- The engine's `main` bookmark always advances to the newest commit — naive
  but sufficient until the merge-queue (Stage 5) defines real branch
  semantics.
