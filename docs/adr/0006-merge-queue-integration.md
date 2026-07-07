# ADR-0006: Merge-queue lands commits via serialized rebase; git refs import back into the op log

- **Status:** Accepted
- **Date:** 2026-07-07
- **Deciders:** Clotho core

## Context

Stage 5 (docs/prd.md §5) is the naive-but-real answer to the genuinely
unsolved problem the vision spec claims as Clotho's moat (§3.1, §7):
N agents committing concurrently to one repository. Until now the engine
carried a placeholder — the `main` bookmark advanced to *every* new commit,
whatever its parent — and ADR-0003 deferred a second gap to this stage:
writes made through Forgejo (UI merges, pushes) moved git refs behind jj's
back, invisible to the op log.

## Decision

**Write-time never blocks; land-time is serialized.**

- `clotho-vcs` `Commit` now only moves `main` when the new commit is a
  descendant of the current main target (a fast-forward). Sibling commits —
  concurrent agents branching from the same base — coexist as jj's
  first-class anonymous heads and leave `main` alone.
- A new engine operation, `IntegrateCommit`, is the only other way `main`
  moves: fast-forward when possible, otherwise rebase the submission onto
  the main target (`jj_lib::rewrite::rebase_commit` — same change id, new
  commit id). A rebase through a conflict **does not stop**: the commit
  lands marked conflicted (jj's unresolved-tree representation, real git
  objects), `main` still advances, and the response names the conflicted
  paths. Resolution is a later commit, not a queue blocker.
- `clotho-merge-queue` graduates from stub to a real gRPC service:
  `SubmitChange(repo, commit_id)` waits its turn on a per-repo async mutex
  and delegates to `IntegrateCommit`. The engine owns repo mutation; the
  queue owns ordering — and stays deliberately dumb (in-process lock, no
  persistence, no batching, no speculative CI), per the PRD §7 warning
  against solving this perfectly.

**Forgejo-side writes flow back through `git::import_refs`.**

- Before every engine operation, `load_repo` diffs the backing git repo's
  refs against the view's bookkeeping and imports any external movement
  (`jj_lib::git::import_refs`, never abandoning unreachable commits). A
  Forgejo UI merge or push that moves `refs/heads/main` becomes an ordinary
  `import git refs` operation — visible in the op log, restorable like
  anything else. This closes ADR-0003's "writes through Forgejo bypass the
  jj op log" consequence.
- The engine records its own ref writes in the view (raw git-ref entry plus
  the `main@git` remote-tracking ref that import diffs against), so imports
  fire only for genuinely external changes and three-way ref merges have
  the correct base.

## Consequences

- Two agents committing concurrently reconcile into one graph with no human
  in the loop; the conflicting case surfaces a clearly-marked conflict
  commit with its paths — both verified by
  `crates/clotho-merge-queue/tests/queue.rs`, external ref-import by
  `crates/clotho-vcs/tests/vcs.rs`.
- The queue is a single point of ordering per repo *per process*: no
  persistence across restarts (an interrupted submission is simply
  re-submitted — integration is idempotent for fast-forwards) and no
  multi-replica story. Fine for the prototype; a real deployment needs a
  shared queue.
- Conflicted commits render in Forgejo as ordinary commits whose files
  contain jj's conflict materialization — legible, if ugly. The PR view
  (Stage 6) should present conflicts properly.
- A write racing into the window between an operation's ref import and its
  ref mirror can still be clobbered in git (though never lost from the jj
  op store). Accepted for the prototype; noted in the engine.
