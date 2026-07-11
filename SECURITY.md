# Security policy

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability or include secrets,
tokens, private repository content, provider responses, or exploit details in
public logs. Use GitHub's private vulnerability-reporting flow:

<https://github.com/pkyanam/clotho/security/advisories/new>

Include the affected commit or release, deployment profile, impact, minimal
reproduction, and whether credentials or untrusted artifacts are involved.
Use synthetic data and redact every credential. If private reporting is not
available, open a public issue containing only a request for a private contact;
do not disclose the vulnerability there.

Maintainers will acknowledge a complete report within five business days,
triage severity, coordinate a fix and disclosure window, and credit reporters
who want attribution. This is an alpha project and does not yet promise a fixed
remediation SLA.

## Supported versions

Until the first tagged public alpha, only the latest commit on `main` receives
security fixes. After tags begin, this table will identify the supported line:

| Version                           | Supported |
| --------------------------------- | --------- |
| latest `main` pre-release         | yes       |
| older commits and untagged images | no        |

## Security boundaries

- Clotho is the public source of truth; Forgejo is an unmodified internal
  provider and its debug UI/API are unsupported product surfaces.
- Human and agent credentials are hashed at rest where applicable, scoped,
  revocable, and must never appear in logs. Stored provider secrets are
  write-only through public APIs.
- Repository files, issues, comments, cards, imports, Actions logs, provider
  output, and tool output are untrusted content and never grant authority.
- Auth, private networking, artifact safety, provider readiness, release
  verification, audit persistence, and destructive actions fail closed.

See [the threat model](./threat-model.md) and
[known limitations](./known-limitations.md) for reviewed threats and current
alpha boundaries.
