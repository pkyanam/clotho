# Stage 22 live-test truth and fixture hygiene

**Date:** July 11, 2026

## Contract

Plain host `cargo test --workspace` may omit service-backed integration tests
when their documented endpoint is absent. Release/CI invocations may not.
`CLOTHO_TEST_FAIL_ON_SKIP=1` converts a missing collaboration, Stage 11
database, storage, or MCP endpoint into a test failure. CI sets the gate, and
`just test-collab`, `just test-storage`, and `just test-agent` set it
themselves.

The collaboration recipe now supplies the Stage 11 database explicitly and
reads the internal Forgejo token from the already-running gateway container
without printing it. CI starts the API gateway with its real control-plane
database and runs Stage 11 under the same fail-on-skip gate. A configured
Stage 11 database that cannot initialize is a failure, never a silent skip.

## Fixture ownership and cleanup

- The Stage 3, Stage 6, and Stage 11 collaboration/control-plane tests in this
  slice create only names prefixed `stage3-`, `stage6-`, or `stage11-repo-`.
  Cleanup refuses every other name and deletes through canonical
  `DELETE /api/v1/repos/{name}`, removing Clotho metadata and the internal
  collaboration-provider project.
- Stage 11 organization cleanup accepts only `stage11-org-` names and removes
  the exact organization and its activity in one database transaction.
- Storage tests use an exact `run-<unique>-<test>` object prefix. Cleanup
  refuses a missing, nested, or non-`run-` prefix, deletes only objects below
  that prefix, and verifies the prefix is empty.
- Cleanup runs after success and after a caught test panic. Set
  `CLOTHO_TEST_KEEP_FIXTURES_ON_FAILURE=1` only while debugging to preserve a
  failed fixture; successful fixtures are always removed.
- The MCP test revokes every token minted for its exact `weaver-` and
  `outsider-` identities, even after a caught panic, and deletes its exact
  `stage4-` repository through Clotho REST. There is no HTTP boundary to
  delete/disable an agent: the two unique agent rows, revoked-token metadata,
  and audit provenance remain durable. Cleanup does not use raw SQL to hide
  that product retention gap.

No Docker volume is removed. Collaboration cleanup currently cannot reclaim
the internal VCS repository directory because Clotho has no public, atomic VCS
delete boundary; the metadata/provider fixture is removed and CI remains
ephemeral, but that bounded internal directory is a known local-stack residue.
A future repository-deletion durability slice should add a Clotho-owned VCS
deletion/reconciliation operation rather than deleting filesystem paths from
tests.

## Verification

```text
cargo fmt --all --check                                      PASS
cargo check -p clotho-api-gateway --tests                    PASS
cargo check -p clotho-storage --test storage                 PASS
cargo clippy -p clotho-api-gateway -p clotho-storage --tests
  -- -D warnings                                             PASS
cargo clippy -p clotho-agent-gateway --test agent
  -- -D warnings                                             PASS
ruby YAML parse + just --dry-run                             PASS

CLOTHO_TEST_FAIL_ON_SKIP=1, collaboration endpoint absent    EXPECTED FAIL (exit 101)
CLOTHO_TEST_FAIL_ON_SKIP=1, Stage 11 database absent         EXPECTED FAIL (exit 101)
CLOTHO_TEST_FAIL_ON_SKIP=1, storage endpoint absent          EXPECTED FAIL (exit 101)
CLOTHO_TEST_FAIL_ON_SKIP=1, MCP endpoint absent              EXPECTED FAIL (exit 101)

just test-collab                                             PASS
  gateway unit tests                                         52 passed
  Clerk integration                                           4 passed
  auth integration                                            3 passed
  collaboration / OpenAPI / Stage 11 / Stage 6               7 passed
  post-run control-plane query                                0 recent test repos, 0 recent test orgs

CLOTHO_STORAGE_TEST_FILE_MB=64 just test-storage             PASS (2 passed)

just test-agent                                              PASS (1 passed)
  post-run control-plane query                               0 recent stage4 repos
  post-run identity query                                    0 active test tokens
  retained provenance                                        2 agent rows
```

The live storage tests themselves listed and verified each removed prefix was
empty. No provider-credential-gated test was represented as executed by these
commands.
