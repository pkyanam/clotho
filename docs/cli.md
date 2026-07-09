# Clotho CLI

`clotho` is a thin human client over the **REST edge** (ADR-0010). It never
shells out to `git` or `jj`.

```bash
cargo run -p clotho-cli -- help
# or after install:
clotho help
```

## Configuration

| Flag / env | Default | Meaning |
|---|---|---|
| `--api <url>` / `CLOTHO_API_URL` | `http://localhost:8080` | API gateway base URL |
| `--json` | off | Pretty-print JSON responses on stdout |

Exit status is non-zero on HTTP or usage errors (script-friendly).

## Demo loop (CLI only)

```bash
export CLOTHO_API_URL=http://localhost:8080

# 1. Create a repo
clotho repo init my-demo
# Stage 8 alias: clotho init my-demo

# 2. Open an issue
clotho issue create my-demo --title "wire up CI" --body "need Actions on main"

# 3. Commit + submit (reads local files; gateway writes real commits)
echo 'fn main() {}' > src/main.rs
clotho repo commit my-demo -m "scaffold" --file src/main.rs --submit

# 4. Pull requests
clotho pr list my-demo
# clotho pr create my-demo --title "…" --head feature --base main
# clotho pr review my-demo 1 --event APPROVE
# clotho pr merge my-demo 1 --method squash

# 5. Actions
clotho actions run my-demo --actor cli
clotho actions list my-demo
# clotho actions logs my-demo <run-id>

# 6. Providers + activity
clotho provider list
clotho activity --limit 10
```

Machine-readable:

```bash
clotho --json issue list my-demo open
clotho --json actions list my-demo
```

## Command groups

| Group | Subcommands |
|---|---|
| `repo` | `init`, `list`, `status`, `log`, `tree`, `commit`, `submit` |
| `issue` | `list`, `create`, `get`, `comment` |
| `pr` | `list`, `create`, `get`, `comment`, `review`, `merge`, `diff` |
| `actions` | `list`, `run`, `get`, `logs`, `config` |
| `provider` | `list`, `get`, `connect` |
| `secret` | `list`, `set`, `get`, `delete` (org\|repo; values write-only) |
| `org` | `list`, `create`, `get`, `repos` |
| `activity` | feed (`--limit N`) |

Stage 8 aliases still work: `init`, `status`, `log`, `commit`, `submit`, and
`pr <repo> [state]` (list).

## Secrets

```bash
clotho secret set org clotho --name DAYTONA_API_KEY --value '…'
clotho secret list org clotho
# Responses show name + last4 only — never the raw value.
```

Or via provider connect:

```bash
clotho provider connect daytona --api-key '…' --org clotho
```
