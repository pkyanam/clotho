## Outcome

Describe the user or agent outcome and the smallest complete slice delivered.

## Contract and compatibility

- REST/OpenAPI behavior:
- SDK/CLI/MCP/web parity:
- Compatibility or deprecation impact:
- Migrations and forward-repair behavior:

## Evidence

List exact commands and results. Separate host tests from live Docker, HTTP,
streaming, restart, browser, recovery, and provider checks. List every skipped
credential-gated check explicitly.

## Do not claim complete unless

- [ ] I preserved unrelated worktree changes, submodules, and Docker volumes.
- [ ] REST, OpenAPI, SDK, CLI, MCP, and web agree for each affected public behavior.
- [ ] Errors, auth, policy, destructive actions, provider readiness, and secrets fail closed.
- [ ] New collections, payloads, logs, queues, retries, and jobs have explicit bounds.
- [ ] Durable behavior has migration, restart, idempotency, and recovery evidence where applicable.
- [ ] Rust/JS checks and relevant disposable live-stack tests pass.
- [ ] Web changes have production-build, keyboard/responsive, both-theme, and real-browser evidence.
- [ ] Documentation, handoff, release-gap status, known limitations, and exact skipped gates are current.
- [ ] The change satisfies the applicable gate in `docs/release-readiness.md`; unit tests alone are not presented as cross-system proof.

## Risk and rollback

State remaining risk, monitoring/reconciliation behavior, and the forward repair
or complete-restore path. Do not propose rewriting published migration or Git
history.
