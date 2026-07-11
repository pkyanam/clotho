# ADR-0017: Merge policy and review threads

## Status

Accepted

## Context

Pull requests are proxied from the collaboration provider ([ADR-0003](0003-forgejo-integration-adopt.md),
[ADR-0011](0011-clotho-collaboration-facade.md)). Stage 9 added create, review,
comment, and merge endpoints without Clotho-owned gates. Commit statuses from
Actions CI already report on PR heads ([ADR-0008](0008-compute-cci-daytona.md)).

Teams need honest merge blockers (conflicts, required checks, required
approvals) configurable per repository without exposing provider-specific
branch-protection APIs in the product surface.

Review discussion today lands as flat issue-style comments. Inline review
threads depend on upstream returning `in_reply_to` / `pull_request_review_id`;
we must not fabricate nesting.

## Decision

1. **Clotho-owned merge policy** — `repo_merge_policies` in Postgres stores
   `require_passing_actions`, `block_merge_when_conflicted` (default true),
   `require_review_approvals`, and `protect_default_branch` (stored; full
   direct-push enforcement deferred). Exposed at
   `GET/PUT /api/v1/repos/{name}/merge-policy`. PUT requires repo admin.

2. **Enforcement at merge** — `POST .../pulls/{number}/merge` loads policy,
   head commit statuses, and submitted reviews, then returns **409** with
   `{ "error": "..." }` when a gate fails. Upstream merge is not attempted
   until checks pass.

3. **Review comments (best-effort)** — `GET .../pulls/{number}/comments`
   returns Clotho `Comment` objects with optional `in_reply_to` and
   `pull_request_review_id` when upstream provides them, merged with flat
   issue-style discussion comments. `POST` accepts optional `in_reply_to` for
   threaded replies when supported. `GET .../pulls/{number}/reviews` lists
   submitted reviews for approval counting and the web review panel.

4. **Web preview** — PR detail disables merge and lists blockers client-side
   using the same policy inputs (policy + mergeable + statuses + reviews).
   Settings → merge hosts the policy form.

5. **Limitations (honest)** — Full GitHub-style branch protection (path
   rules, push allow-lists, required reviewers per file) is out of scope.
   `protect_default_branch` is persisted for a follow-up. Inline threads only
   render when real `in_reply_to` metadata exists.

## Consequences

- SDK/CLI/OpenAPI gain `getMergePolicy`, `updateMergePolicy`,
  `listPullComments`, and `listPullReviews`.
- Merge failures surface clear 409 reasons in CLI (`clotho pr merge`) and web.
- Slice F can demo the full loop: set policy → open PR → run actions → review
  → merge blocked/allowed with visible reasons.
