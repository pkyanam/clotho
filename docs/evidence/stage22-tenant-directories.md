# Stage 22 tenant-directory authorization evidence

**Date:** July 11, 2026

`GET /api/v1/users` and `GET /api/v1/orgs` now resolve the authenticated human
and filter in SQL by shared organization membership. `GET /api/v1/orgs/{org}`
applies the same membership predicate before loading the roster; foreign and
absent organizations both return the stable `404 not_found` envelope.

Verification:

- `cargo test -p clotho-api-gateway control::tests::user_and_org_directories_are_membership_scoped -- --exact --nocapture`
- `CLOTHO_AUTH_TEST_DATABASE_URL=postgres://clotho:clotho-dev@localhost:5432/clotho CLOTHO_TEST_FAIL_ON_SKIP=1 cargo test -p clotho-api-gateway --test private_repo_reads -- --nocapture`
- `cargo fmt --all --check`
- `cargo clippy -p clotho-api-gateway --all-targets -- -D warnings`

The live-Postgres adversarial test creates two humans and two organizations,
proves each directory excludes the foreign tenant, proves the owning org roster
is readable, and proves the foreign roster is concealed. Fixtures are removed
after the test.
