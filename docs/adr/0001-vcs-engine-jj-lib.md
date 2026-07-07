# ADR-0001: Build the VCS engine on jj-lib

- **Status:** Accepted
- **Date:** 2026-07-07
- **Deciders:** Clotho core

## Context

Clotho's central bet is that version control must serve humans and AI agents
working concurrently on the same repository. Raw git plumbing was designed for
neither concurrent machine writers nor supervised recovery: it has a staging
area agents forget to use, blocking conflicts that deadlock parallel writers,
and no first-class record of repository operations to restore from.

Jujutsu (jj) offers a different model while remaining fully git-compatible:

- **Operation log as a first-class API.** Every operation (commit, pull,
  rebase, undo) is a queryable log entry — exactly the primitive a supervising
  system needs for `checkpoint` / `restore_to` semantics exposed as API calls.
- **No staging area; working-copy-as-commit.** Removes the entire class of
  "agent forgot to `git add`" failures.
- **Conflicts as first-class, non-blocking objects.** A rebase through a
  conflict lands marked conflicted and is resolved later — essential when N
  agents commit concurrently.
- **Git-compatible storage backend.** jj reads/writes real `.git` directories,
  so every commit Clotho produces is a real git commit; downstream tooling
  (Forgejo, CI, human muscle memory) works unmodified.

The engine must be a **service** serving many users, not a CLI. `jj-lib` is
explicitly designed to be embedded in a server context, with `gitoxide`
providing the git object backend.

An alternative considered: shelling out to the `jj` binary. Rejected — process
overhead per request, no typed API, fragile output parsing, and no way to hold
repo state across calls. Third-party wrappers (e.g. `agentic-jujutsu`) were
reviewed as interface inspiration only; their production-readiness claims are
unverified and we will not take them as core dependencies.

## Decision

`crates/clotho-vcs` embeds **`jj-lib`** directly (git backend via `gitoxide`)
and exposes the engine as a gRPC service: `init_repo`, `commit`, `checkpoint`,
`restore_to`, `query_op_log`. No component shells out to the `jj` or `git`
binaries at runtime.

One jj **workspace per agent** is the default concurrency model; a merge-queue
service (`crates/clotho-merge-queue`) reconciles workspaces back into the
shared graph. jj's working-copy model is single-writer per workspace, and
real-world testing shows two agents in one workspace can corrupt each other's
commits — the orchestration layer above jj is Clotho's own novel engineering.

## Consequences

- Every Clotho commit is a real git commit; round-trip compatibility with
  GitHub/GitLab workflows is testable from day one (Stage 1 exit condition).
- `jj-lib` is pre-1.0 and experimental: **pin an exact version**, track
  upstream changes deliberately, never auto-upgrade. API churn is expected and
  budgeted for.
- Checkpoint/restore become cheap API calls over the op log rather than
  reflog/stash tricks.
- We own the unsolved multi-workspace reconciliation problem (docs/prd.md §5
  Stage 5, vision spec §7.1) instead of inheriting a solution.
