#!/usr/bin/env bash
# Clotho — Stage 7 end-to-end definition-of-done demo (docs/prd.md §1).
#
# One command, reproducible from a clean stack:
#   1. `just dev` (or `docker compose -f docker-compose.dev.yml up`) in another
#      terminal, wait until the services are healthy;
#   2. put your Daytona key in `.env` (see .env.example) for the CI leg;
#   3. `just demo` (or `./scripts/demo/run.sh`).
#
# This is a thin wrapper: it loads `.env` and runs the clotho-demo driver, which
# talks to the running stack over its real gRPC/REST APIs.
set -euo pipefail

cd "$(dirname "$0")/../.."

# Load .env if present (DAYTONA_API_KEY, CLOTHO_WEBHOOK_SECRET, overrides).
if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

if [ -z "${DAYTONA_API_KEY:-}" ]; then
  echo "note: DAYTONA_API_KEY is not set — the CI leg will report 'error'."
  echo "      Set it in .env to run CI on the real Daytona sandbox provider."
  echo
fi

export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
exec cargo run --quiet -p clotho-demo
