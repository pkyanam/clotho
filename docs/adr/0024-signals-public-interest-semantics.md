# ADR-0024: Signals as permission-safe public repository interest

- **Status:** Accepted
- **Date:** 2026-07-11
- **Deciders:** Clotho core
- **Related:** ADR-0023, Stage 26 Lachesis evidence graph

## Context

Clotho needs a lightweight community primitive analogous to the useful part of
GitHub stars: a person can remember and publicly express interest in a project,
and maintainers can see community attention. Copying an undifferentiated star
counter would conflate bookmarking, notifications, popularity, trust, and real
adoption. It could also leak private repository existence or become accidental
authority for agents and release policy.

Clotho's evidence graph will eventually know stronger relationships—dependents,
builds, releases, and deployments. A human click must not masquerade as that
evidence.

## Decision

### 1. The primitive is named Signal

One authenticated human or organization may Signal a repository it can see.
A Signal has an optional intent:

- `interested` — lightweight interest and the default;
- `using` — a self-declared use relationship;
- `building_on` — a self-declared downstream relationship.

The action is reversible and idempotent. Public API semantics are REST-first;
SDK, CLI, MCP metadata reads, and web follow the same visibility rules.

### 2. Keep four concepts separate

| Concept                   | Meaning                                                      | Authority/trust               |
| ------------------------- | ------------------------------------------------------------ | ----------------------------- |
| Signal                    | public, self-declared interest/intent                        | none                          |
| Follow                    | private notification preference                              | none                          |
| Evidence-derived adoption | dependents, builds, releases, deployments linked by Lachesis | derived, inspectable evidence |
| Vouch (reserved)          | possible future signed endorsement of an immutable release   | undefined until a later ADR   |

Signals never grant repository access, change ranking in policy decisions,
satisfy release gates, authorize agents, or count as verified adoption. Imported
text and agent output cannot create a Signal without the principal's explicit
scope and a normal authenticated mutation.

### 3. Privacy, abuse, and lifecycle precede discovery

- Private repository Signals and aggregates never reveal repository existence,
  membership, or counts to unauthorized principals.
- Deletion, suspension, visibility changes, organization removal, moderation,
  audit, export, and retention have explicit behavior.
- Aggregates are permission-aware and cannot be inflated through replay; one
  principal has at most one active Signal per repository.
- Typed intent is self-declared and displayed as such. Lachesis-derived adoption
  is presented separately.
- Global ranking, trending, recommendation, and federation are deferred until
  tenant isolation, abuse resistance, moderation, and lifecycle semantics are
  proven.

## Consequences

- Signals land with Stage 26 evidence/public-trust work, after Stages 23–24
  establish identity, tenancy, quotas, and operational safety.
- Following is not implemented as a Signal side effect; users can Signal without
  notifications and Follow without public endorsement.
- The initial UI can remain a one-click `Signal` action with optional intent,
  while repository pages show permission-safe totals and explicitly label
  evidence-derived adoption.
- “Vouch” remains available for a future stronger, signed release-level claim;
  the product must not overload Signal with that meaning.
