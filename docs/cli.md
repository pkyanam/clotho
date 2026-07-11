# Clotho CLI

`clotho` is a thin human client over the **REST edge** (ADR-0010). It never
shells out to local `git` binaries — all reads and writes go through the gateway.

> **Pre-release CLI:** command groups are usable today, but grammar and exit-code
> compatibility are not frozen. The public-alpha gate adds generated command
> reference, named contexts with OS-keychain tokens, strict stdout/stderr and
> JSON behavior, stable exit-code classes, completions, signed binaries, and
> retry/idempotency controls. See
> [`release-readiness.md`](release-readiness.md#cli).

```bash
cargo run -p clotho-cli -- help
# or after install:
clotho help
```

## Configuration

| Flag / env | Default | Meaning |
|---|---|---|
| `--api <url>` / `CLOTHO_API_URL` | `http://localhost:8080` | API gateway base URL |
| `--token <tok>` / `CLOTHO_TOKEN` | unset | Bearer token for authenticated routes |
| `--json` | off | Pretty-print JSON responses on stdout |

When `CLOTHO_AUTH_REQUIRED=true` on the gateway, set `CLOTHO_TOKEN` (from
bootstrap logs or `clotho auth token create`) before mutating commands.

**Auth providers (Stage 17):** local/demo use `CLOTHO_AUTH_PROVIDER=bootstrap`
(default) with `clotho_tok_…`. Managed deploy uses `clerk` — pass a Clerk
session JWT or org API key as `--token` / `CLOTHO_TOKEN`, or keep minting
Clotho `clotho_tok_…` (§11 #7). Agents stay on `clotho_agt_…` via MCP only.

Exit status is non-zero on HTTP or usage errors (script-friendly). Gateway error
envelopes (`{ "error": "…" }`) are surfaced in stderr-style messages, including
merge **409** policy blocks from `clotho pr merge`.

For current automation, pin the Clotho release, pass `--json`, and do not parse
human-formatted tables. The compatibility freeze will reserve stdout for the
requested value, stderr for diagnostics/progress, and publish distinct exit
classes for usage, authentication, permission, policy conflict, not-found,
retryable unavailability, and internal failure.

## Demo loop

Copy-paste end-to-end path (verified against `clotho help`):

```bash
export CLOTHO_API_URL=http://localhost:8080
export CLOTHO_TOKEN=clotho_tok_…   # from clotho auth token create or bootstrap logs

clotho auth whoami
clotho repo init demo-loop --kind model
clotho issue create demo-loop --title "flaky" --label bug
clotho pr create demo-loop --title "fix" --head feature --base main
clotho actions run demo-loop --actor cli
clotho actions list demo-loop
# copy run id from list output, then:
clotho actions logs demo-loop <run-id>
clotho provider list
clotho secret list org clotho
clotho activity
```

Optional extensions on the same repo:

```bash
clotho label create demo-loop --name bug --color d73a4a
clotho milestone create demo-loop --title "v0.1"
clotho notification list --unread
clotho repo merge-policy get demo-loop
clotho repo update demo-loop --description "demo" --large-file-threshold 1048576
```

Machine-readable:

```bash
clotho --json issue list demo-loop open
clotho --json actions list demo-loop
```

## Command groups

| Group | Subcommands |
|---|---|
| `auth` | `whoami`, `token create\|list\|revoke` |
| `repo` | `init`, `list`, `status`, `log`, `tree`, `artifacts`, `preview`, `import-hf`, `commit`, `submit`, `update`, `delete`, `merge-policy get\|set` |
| `issue` | `list`, `create`, `get`, `comment`, `update` |
| `label` | `list`, `create` |
| `milestone` | `list`, `create` |
| `notification` | `list`, `read` |
| `pr` | `list`, `create`, `get`, `comment`, `review`, `merge`, `diff` |
| `actions` | `list`, `run` (alias `start`), `get`, `logs`, `config` |
| `provider` | `list [--layer …\|--all]`, `get`, `connect` |
| `secret` | `list`, `set`, `get`, `delete` (org\|repo; values write-only) |
| `org` | `list`, `create`, `get`, `repos` |
| `activity` | feed (`--limit N`) |
| `agent` | `list`, `create`, `tokens`, `mint`, `revoke`, `audit` (org admin) |

Stage 8 aliases still work: `init`, `status`, `log`, `commit`, `submit`, and
`pr <repo> [state]` (list).

## Auth

```bash
clotho auth whoami
clotho auth token create --name "laptop"
clotho auth token list
clotho auth token revoke <id>
```

## Providers (fabric)

```bash
clotho provider list                          # compute (default)
clotho provider list --layer auth
clotho provider list --layer storage          # live Arachne + StorageSDK state
clotho provider list --layer network          # live Tailscale connection state
clotho provider list --layer hub              # Hugging Face import path
clotho provider list --all
```

Tailscale and repo network policy:

```bash
clotho provider connect tailscale --client-id '…' --client-secret '…' --org clotho
clotho repo update my-demo --network tailscale --network-tag tag:clotho-my-demo
clotho provider disconnect tailscale --org clotho
```

Clotho live-verifies the OAuth client before encrypting it. Repositories marked
`tailscale` fail closed until both network credentials and a private-net-capable
compute path are ready.

GPU Actions policy:

```bash
clotho actions config model-repo --provider daytona --accelerator gpu --gpu-type H100 --gpu-type H200
clotho actions run model-repo
```

CCI validates that the chosen provider advertises GPU support. Daytona jobs use
its `daytona-gpu` snapshot; CPU remains the default for every repository.

## Agents

Manage non-human identities (requires org admin or bootstrap; gateway needs
`CLOTHO_AGENT_ADMIN_TOKEN`):

```bash
clotho agent list
clotho agent create weaver --description "demo agent"
clotho agent mint weaver --repos weave,demo-loop --tools '*'
clotho agent tokens weaver
clotho agent revoke weaver <token_id>
clotho agent audit weaver --limit 20
```

Mint output includes the plaintext `clotho_agt_…` token once — save it before
closing the terminal. Agent mint is **CLI/web only**; MCP has no admin tools.

## Repo settings

```bash
clotho repo update my-demo --description "…" --visibility private --default-branch main --kind dataset --large-file-threshold 1048576
clotho repo merge-policy set my-demo --require-actions --approvals 1
clotho repo delete my-demo --yes
```

Repository kinds are `code`, `model`, and `dataset`. When no threshold is
provided, Clotho uses 10 MiB for code and 1 MiB for model/dataset artifacts;
no environment variable is required.

Inspect model/dataset formats, logical sizes, Arachne placement, and card /
metadata / primary-artifact readiness directly from the Clotho control plane:

```bash
clotho repo artifacts my-model
clotho --json repo artifacts my-dataset
clotho repo preview my-dataset data/train.jsonl --limit 25
clotho repo import-hf my-model hf-internal-testing/tiny-random-gpt2 --revision main
clotho repo imports my-model
clotho repo release my-model v1.0.0
clotho repo releases my-model
clotho actions run my-model --workflow evaluate --release v1.0.0
```

Use repeated `--path` flags for a selective snapshot. The source accepts
`namespace/name`, `namespace/name@revision`, or a `https://huggingface.co/...`
repository URL. Imports default to 200 files / 50 GiB, block unsafe Hub scanner results, stream large files directly
to Arachne, and land through the merge queue. `--allow-unsafe` is an explicit
CLI-only override. Private/gated repositories use a Clotho-stored
`HUGGINGFACE_TOKEN`; public repositories need no credential.
The import command queues a durable job and returns immediately; `repo imports`
shows live file/byte progress, commit status, and terminal errors.

## Secrets

```bash
clotho secret set org clotho --name DAYTONA_API_KEY --value '…'
clotho secret list org clotho
clotho secret set repo my-demo --name DEPLOY_KEY --value '…'
clotho secret list repo my-demo
# Responses show name + last4 only — never the raw value.
```

Or via provider connect:

```bash
clotho provider connect daytona --api-key '…' --org clotho
```
