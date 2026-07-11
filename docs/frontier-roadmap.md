# Clotho frontier roadmap

This document is the “dream, then sequence it” companion to the PRD. Ideas are
included only when they reinforce Clotho's source-of-truth position, create a
defensible advantage over GitHub/GitLab/Hugging Face, and can be delivered on
top of existing primitives without turning the platform into an unrelated
collection of features.

## North star

**A Clotho repository is a living, verifiable system—not a folder of files.**

It binds source, artifacts, data lineage, agents, compute, network reach,
evaluations, policy, and releases to one recoverable history. A human or agent
should be able to answer “what is this, why should I trust it, what produced it,
where can it run, and how do I continue the work?” without consulting another
control plane.

## Priority order

| Horizon | Outcome | Why now |
|---|---|---|
| Now | Public-alpha hardening and agent-ready handoff | Trust must precede surface growth |
| Next | Production identity, authorization, and tenant isolation | Hosted scale and safe multi-user self-hosting need the same hard boundary |
| Next | Hosted control plane and elastic workload cells | Durable state, fair scheduling, and autoscaling must be designed separately |
| Next | Handoff Capsules and repository task plane | Makes multi-agent continuity a native primitive |
| Next | Evidence Graph, release policy, and Signals | Turns provenance into a moat while adding permission-safe community interest |
| Next | Compute Bindings and GPU locality | Makes “GPU attached to a repo” useful and economical |
| Later | Lazy virtual repositories and protocol mesh | Removes migration and large-checkout friction |
| Later | Open provider/plugin kit and federation | Converts modular architecture into an ecosystem |

## 1. Production identity and tenant isolation

Clotho's hosted and self-hosted forms must share one identity and authorization
model. An organization is the immutable security, policy, quota, storage,
provider, audit, and workload boundary. Clerk, OIDC, and local bootstrap are
authentication adapters; Clotho memberships, roles, grants, agent scopes, and
policy remain authoritative.

Tenant context must survive every transition: REST to internal services,
database query, durable job, webhook, cache, object key, log, audit row, provider
call, and asynchronous completion. PostgreSQL row-level security adds defense in
depth. It does not excuse global application queries or inferred tenants.

**Viable first slice:** inventory every durable table and stable route, add a
typed `TenantContext` plus deny-by-default route/resource matrix, then migrate
one complete high-risk family such as secrets or artifacts with adversarial
cross-org tests.

**Success:** two hostile organizations cannot infer or affect one another by
direct ID, timing, pagination, cache, storage, queue, webhook, provider, agent,
or background-work paths.

## 2. Hosted control plane and elastic workload cells

Clotho should scale accepted work without confusing stateless replication with
stateful correctness. API, MCP, and web replicas can scale horizontally. Jobs
are persisted before acknowledgement and scheduled through durable leases,
idempotent reconciliation, tenant quotas, fair admission, and bounded retries.
Disposable workload cells scale by queue pressure, capability, region, and
provider capacity. Postgres, Git/VCS, merge coordination, Arachne, and lifecycle
ownership require explicit HA, sharding, backup, restore, and failover designs.

One source tree supports Compose for evaluation/small self-hosting and a tested
Helm/Kubernetes production profile with external Postgres and object storage.
Metering, quotas, SLOs, traces, alerts, rolling upgrades, restore drills, signed
artifacts, and incident procedures are product requirements—not hosted-only
tribal knowledge.

**Viable first slice:** persist one common operation/lease contract, run multiple
stateless gateways against it, and autoscale a fake workload-cell fleet while
proving tenant fairness, retry idempotency, and accepted-work survival.

**Success:** killing a gateway or worker and performing a rolling upgrade loses
no accepted work; workload capacity scales with demand; a noisy tenant cannot
starve another; and a second operator restores a representative deployment.

## 3. Handoff Capsules — resumable work as a versioned object

An agent handoff is currently prose plus implicit workspace state. Clotho should
make it a first-class, immutable object bound to a repository operation.

A capsule contains:

- objective, constraints, acceptance tests, and remaining work;
- exact base/head operations, working diff, checkpoint, and conflict state;
- agent identity, tool scopes, budget/lease, and execution provenance;
- referenced issues, PRs, Actions, releases, datasets, and logs;
- a bounded context manifest (paths/symbols/decisions), not a giant transcript;
- claims that are proven, assumed, failed, or require human authority.

Another agent can inspect, fork, resume, or reject the capsule. A human can see
the same object as a concise status page. Capsules build directly on jj
checkpoints, agent identity, durable jobs, audit, and structured diffs.

**Viable first slice:** read-only capsule creation from an existing checkpoint
and audit window, exportable as JSON/Markdown, then an MCP `resume_handoff`
operation that validates the base operation and token scope.

**Success:** a fresh agent resumes a partially completed change and reaches the
same test outcome without transcript access or repository archaeology.

## 4. Repository Task Plane — issues that can safely execute

Add a Clotho-native task resource above issues: desired outcome, authority,
budget, required capabilities, dependencies, policy, and terminal conditions.
Tasks lease an isolated workspace to a human or agent and produce capsules,
commits, Actions, or explicit blockers.

Useful differentiators:

- dependency graph and concurrency-safe leases;
- “read / propose / implement / deploy” authority levels;
- automatic checkpointing and bounded retries;
- review routing based on affected ownership, risk, and agent reliability;
- speculative parallel attempts whose evidence can be compared before landing;
- no silent success: acceptance evidence is attached to the task.

This is intentionally not a generic project-management clone. It is the
transaction layer between intent and repository mutation.

## 5. Lachesis Evidence Graph — trust you can query

Promote the reserved **Lachesis** name into a measurement and evidence layer.
Every release becomes a graph linking:

- source commits and agent/human provenance;
- model weights, datasets, schemas, cards, licenses, and base models;
- training/evaluation code and immutable inputs;
- compute provider, accelerator type, image, environment, duration, and cost;
- test, benchmark, safety, security, and policy results;
- signatures, SBOMs, attestations, approvals, and deployment consumers.

The graph is not a manually edited dashboard. Edges derive from Clotho-owned
operations and are content-addressed. REST, CLI, MCP, and web can answer:

- Which datasets and code produced this weight file?
- Which releases are affected by a vulnerable dependency or revoked dataset?
- Is this benchmark comparable to the previous release?
- Which agent-authored changes reached a deployed artifact without human review?

**Viable first slice:** a release evidence manifest composed from existing
semantic manifests, Actions provenance, commit audit, and approvals, plus an
impact query by digest.

### Signals — interest without counterfeit trust

Signals are Clotho's lightweight public repository-interest primitive. One
authenticated human or organization may Signal a visible repository, optionally
declaring `interested`, `using`, or `building_on`. Signals are self-declared,
grant no authority, never satisfy release policy, and never count as verified
adoption. Private Following remains a separate notification preference.

Lachesis independently derives stronger relationships such as dependents,
builds, releases, and deployments. UI and APIs must never merge these evidence
facts into the Signal count. Global ranking and federation wait for permission,
moderation, abuse, deletion, and lifecycle semantics.

**Viable first slice:** idempotent Signal/un-Signal for public repositories,
permission-safe aggregate counts, typed intent, audit, deletion behavior, and a
web control. Private repositories receive explicit non-disclosure tests.

## 6. Executable release policy and evaluation contracts

Model cards and CI YAML describe intent loosely. Add typed repository contracts
for release eligibility:

- required artifacts and metadata;
- evaluation datasets, metrics, thresholds, and permitted regressions;
- security/license/scanner requirements;
- approved compute/network/data-boundary capabilities;
- human-review rules for machine-authored changes;
- reproducibility level (best effort, pinned, hermetic, independently verified).

The web UI should generate common policies; users should not need to hand-write
configuration for ordinary workflows. Policies are evaluated against Lachesis
evidence and return explainable, machine-readable failures.

## 7. Compute Bindings — attach capability, not a vendor

Treat compute as a repository resource with lifecycle and locality, not merely
an Actions dropdown. A binding declares capability such as `gpu:h100`, trusted
network reach, region, persistence, data residency, and budget. CCI resolves it
to Daytona, ComputeSDK, BYOC, or a future provider.

Creative but practical workflows:

- **release-warm GPU:** materialize and verify a release once into a persistent
  sandbox/snapshot, then fork cheap inference/evaluation sessions from it;
- **branch accelerator:** attach a leased GPU workspace to a branch or capsule,
  automatically checkpointing workspace and Arachne outputs;
- **data-local scheduling:** choose compute near the configured object store or
  inside the tailnet, minimizing weight/dataset transfer;
- **GPU cache lineage:** cache entries are keyed by release manifest, runtime
  image, driver/CUDA contract, and architecture—never by mutable filenames;
- **verified result return:** outputs land in an isolated Arachne namespace and
  become repository changes only through policy and the merge queue;
- **cost/risk preview:** show predicted transfer, warmup, accelerator time, and
  trust boundary before execution.

**Viable first slice:** persistent release materialization on Daytona plus a
binding resource that resolves one release and one accelerator capability.

## 8. Lazy virtual repositories

Expose a repository filesystem that materializes metadata immediately and
fetches Arachne ranges only when read. This enables near-instant checkouts for
hundreds-of-gigabyte model/dataset repositories and keeps sandboxes small.

The same manifest can support:

- a FUSE-style local mount;
- a sandbox-side lazy materializer;
- sparse path/symbol checkout for agents;
- cache sharing across workspaces without sharing write authority;
- offline “travel packs” containing selected files, history, tools, and evidence.

Start inside managed sandboxes, where the environment is controlled, before
shipping a cross-platform desktop mount.

## 9. Protocol mesh — one release, many ecosystems

Clotho already projects releases through Hugging Face read APIs. Generalize the
projection layer so one verified release can be consumed as:

- Git and Git LFS;
- Hugging Face Hub;
- OCI artifacts/images and provenance referrers;
- static HTTPS/range-addressable artifacts;
- optionally S3-compatible read views and package registries.

The underlying identity remains the Clotho release digest and evidence graph.
This is a migration advantage: teams can adopt Clotho as the control plane
without rewriting every downstream client on day one.

## 10. Repository-bound data connectors

Connect external databases, warehouses, vector stores, and object catalogs as
scoped repository context without copying their contents into Git. Agents see
schema, bounded samples, lineage, and approved query tools through Clotho.

Requirements:

- capability-scoped, read-only by default;
- query limits, redaction, audit, and network policy;
- schema snapshots committed as evidence without committing live data;
- reproducible query/result references tied to a release or task;
- connector credentials remain in Clotho's vault and are never prompt context.

This makes the repository describe the system it operates, not just its source.

## 11. Atropos retention and verifiable deletion

Use the reserved **Atropos** name for explicit lifecycle policy: retention,
legal hold, garbage collection, cache eviction, credential revocation, and
verifiable deletion across Git, Arachne, backups, and providers.

Deletion should produce a signed tombstone/evidence record without retaining
the deleted secret or content. This is necessary once global dedup, forks, and
external stores coexist.

## 12. Open provider and policy kit

Turn internal modularity into a supported extension ecosystem:

- versioned provider conformance suites for compute, storage, network, auth,
  Hub, and data connectors;
- out-of-process adapters with capability discovery and health contracts;
- signed manifests, least-privilege secrets, and explicit egress declarations;
- policy packs and UI schemas so integrations do not require core-web changes;
- compatibility badges based on automated conformance, not partner marketing.

Clotho should remain fully useful with its default open stack; plugins extend
capability rather than complete an intentionally hollow product.

## Ideas deliberately deferred

- General chat, documents, or ticketing unrelated to repository state.
- Cryptocurrency/token incentives for contributions.
- Autonomous production deployment before task authority and evidence policy.
- Semantic/AST merge as a marketing claim before durable runtime correctness.
- Global Signal ranking before permission-safe aggregation, abuse controls,
  moderation, and verified public artifacts.
- A proprietary model-serving API that duplicates mature inference platforms;
  Clotho should bind and verify deployments rather than own every runtime.

## Decision rule

A proposed feature belongs in Clotho only if it strengthens at least two of:

1. repository source-of-truth integrity;
2. human/agent concurrency and handoff;
3. artifact/data/model portability;
4. verifiable provenance and policy;
5. modular infrastructure ownership;
6. performance or operational simplicity at large scale.

Otherwise it should be an integration, not core platform scope.
