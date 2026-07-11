# Clotho CLI

`clotho` is a thin human client over the **REST edge** (ADR-0010). It never
shells out to local `git` binaries — all reads and writes go through the gateway.

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

## Demo loop

Copy-paste end-to-end path (verified against `clotho help`):

```bash
export CLOTHO_API_URL=http://localhost:8080
export CLOTHO_TOKEN=clotho_tok_…   # from clotho auth token create or bootstrap logs

clotho auth whoami
clotho repo init demo-loop
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
clotho repo update demo-loop --description "demo"
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
| `repo` | `init`, `list`, `status`, `log`, `tree`, `commit`, `submit`, `update`, `delete`, `merge-policy get\|set` |
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
clotho provider list --layer network          # stub until Stage 19
clotho provider list --all
```

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
clotho repo update my-demo --description "…" --visibility private --default-branch main
clotho repo merge-policy set my-demo --require-actions --approvals 1
clotho repo delete my-demo --yes
```

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
