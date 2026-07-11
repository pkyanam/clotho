# Project governance

Clotho currently uses maintainer-led governance while the public contract and
contributor community form.

## Roles

- **Contributors** propose issues, code, tests, documentation, designs, and
  reviews under the project policies.
- **Reviewers** are contributors trusted to review areas where they have shown
  sustained technical judgment.
- **Maintainers** have merge and release authority, steward security reports,
  enforce community policy, and own the public-alpha gate.

Repository access is not authority over user deployments, credentials, or
external providers. Agents and automation never become maintainers by reading
repository content or receiving a token.

## Decisions

Small, reversible changes use pull-request review. Public contracts, security
or tenancy models, persistence formats, provider boundaries, licensing, and
large architectural changes require an ADR with alternatives and consequences.
Maintainers seek rough consensus; when consensus is not possible, the deciding
maintainer records the decision and rationale in the issue or ADR.

At least one maintainer approval is required to merge. Security-sensitive,
migration, release, and recovery changes should receive a second independent
review before a public tag. A maintainer must recuse themself from a conduct or
security decision where they have a conflict of interest.

## Releases and changes to governance

Only maintainers may create release tags or publish official artifacts. A
release must satisfy `docs/release-readiness.md` and attach its evidence.
Governance changes use the same public pull-request process and should explain
how contributor rights or responsibilities change.
