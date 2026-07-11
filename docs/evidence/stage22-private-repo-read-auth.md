# Stage 22 private-repository read authorization

**Date:** July 11, 2026

The API gateway now resolves repository visibility from the Clotho control
plane before any VCS, storage, compute, agent-gateway, or collaboration-provider
read. Public repositories allow anonymous reads. A supplied malformed,
expired, or invalid human bearer always returns `401`; it is never ignored on
a public route. Private and internal repositories require explicit human read
permission or org-admin authority. A missing repository, a globally ambiguous
name, and an unauthorized non-public repository all return the same stable
`404 not_found` / `repository not found` envelope.

The common gate covers repository detail, tree/file/artifact/storage/commit/op
reads; issues, labels, milestones, pulls/reviews/diffs, branches and statuses;
Actions runs/logs/config; Hub import records; releases and downloads; merge
policy; and agent sessions. Hugging Face model/dataset projections use the same
gate, including their delegated release-download path.

Global and org repository lists filter visibility/permission and ambiguous
names in Postgres before cursor pagination. Activity applies the same rule in
SQL before its page limit; org-only events require membership. Name-only
lookup fetches at most two rows and denies duplicates. New repository creation
rejects any existing global name before its first VCS/provider call while
public routes remain name-only.

Adversarial evidence in `tests/private_repo_reads.rs` uses two humans, two
organizations, public/private/internal repositories, and deliberately
duplicate names under `CLOTHO_AUTH_REQUIRED=true`. It exercises 32 name-routed
read shapes, stable foreign-vs-missing responses, invalid credentials, list
and activity pagination, open-local bootstrap fallback, public Hugging Face
double authorization, and pre-side-effect duplicate creation. Exact fixtures
are transactionally removed.

```text
cargo check -p clotho-api-gateway --tests                         PASS
cargo test -p clotho-api-gateway --tests                         PASS
CLOTHO_TEST_FAIL_ON_SKIP=1 CLOTHO_STAGE11_TEST_DATABASE_URL=… \
  cargo test -p clotho-api-gateway --test private_repo_reads     PASS
cargo clippy -p clotho-api-gateway --tests -- -D warnings        PASS
cargo fmt --all --check                                          PASS
git diff --check                                                 PASS
```

This slice deliberately excludes secret metadata authorization (separate
evidence), MCP agent-bearer forwarding, and organization/user directory
visibility. Repository deletion still cannot atomically reclaim internal VCS
directories because no Clotho-owned VCS deletion boundary exists.
