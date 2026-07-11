#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODE="human"
CHECK_STACK=0

for arg in "$@"; do
  case "$arg" in
    --json) MODE="json" ;;
    --stack) CHECK_STACK=1 ;;
    -h|--help)
      printf '%s\n' 'Usage: scripts/doctor.sh [--json] [--stack]'
      printf '%s\n' '  --json   emit exactly one machine-readable JSON value'
      printf '%s\n' '  --stack  require the running Docker and HTTP surfaces'
      exit 0
      ;;
    *)
      printf 'unknown option: %s\n' "$arg" >&2
      exit 2
      ;;
  esac
done

names=()
statuses=()
messages=()
fixes=()
failures=0
warnings=0

record() {
  names+=("$1")
  statuses+=("$2")
  messages+=("$3")
  fixes+=("$4")
  case "$2" in
    fail) failures=$((failures + 1)) ;;
    warn) warnings=$((warnings + 1)) ;;
  esac
}

require_command() {
  local name="$1"
  local probe="$2"
  local fix="$3"
  if ! command -v "$name" >/dev/null 2>&1; then
    record "$name" fail "not found" "$fix"
    return
  fi
  local version
  version="$($probe 2>&1 | head -n 1)"
  record "$name" pass "$version" ""
}

cd "$ROOT" || exit 1

require_command git "git --version" "Install Git 2.x or newer."
require_command rustc "rustc --version" "Install Rust stable with rustup."
require_command cargo "cargo --version" "Install Rust stable with rustup."
require_command protoc "protoc --version" "Install protobuf/protoc."
require_command node "node --version" "Install Node.js 20 or newer."
require_command pnpm "pnpm --version" "Enable Corepack or install pnpm 11.9.0."
require_command just "just --version" "Install the just command runner."
require_command docker "docker --version" "Install Docker Desktop or a Compose-compatible Docker engine."

if command -v docker >/dev/null 2>&1; then
  if docker info >/dev/null 2>&1; then
    record docker-daemon pass "reachable" ""
  else
    record docker-daemon fail "not reachable" "Start Docker Desktop or the Docker daemon."
  fi
  if docker compose version >/dev/null 2>&1; then
    record docker-compose pass "$(docker compose version 2>&1 | head -n 1)" ""
  else
    record docker-compose fail "plugin not available" "Install Docker Compose v2."
  fi
fi

if git submodule status 2>/dev/null | grep -q '^-'; then
  record forgejo-submodule fail "not initialized" "Run: git submodule update --init --recursive"
else
  record forgejo-submodule pass "initialized at $(git -C collab/forgejo rev-parse --short HEAD 2>/dev/null || printf unknown)" ""
fi

if docker compose -f docker-compose.dev.yml config --quiet >/dev/null 2>&1; then
  record compose-config pass "valid without a .env file" ""
else
  record compose-config fail "docker-compose.dev.yml did not validate" "Run: docker compose -f docker-compose.dev.yml config"
fi

if test -n "$(git status --porcelain 2>/dev/null)"; then
  record worktree warn "contains local changes; preserve them" "Review: git status --short"
else
  record worktree pass "clean" ""
fi

if test "$CHECK_STACK" -eq 1; then
  if docker compose -f docker-compose.dev.yml ps --status running --services 2>/dev/null | grep -qx 'clotho-api-gateway'; then
    record stack pass "core containers are running" ""
  else
    record stack fail "api gateway container is not running" "Run: docker compose -f docker-compose.dev.yml up -d --build"
  fi
  if curl --fail --silent --show-error http://localhost:8080/healthz >/dev/null 2>&1; then
    record rest pass "http://localhost:8080/healthz" ""
  else
    record rest fail "gateway health check failed" "Inspect: docker compose -f docker-compose.dev.yml logs clotho-api-gateway"
  fi
  if curl --fail --silent --show-error http://localhost:3100 >/dev/null 2>&1; then
    record web pass "http://localhost:3100" ""
  else
    record web fail "web console check failed" "Inspect: docker compose -f docker-compose.dev.yml logs clotho-web"
  fi
fi

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  printf '%s' "$value"
}

if test "$MODE" = json; then
  if test "$failures" -eq 0; then overall=ready; else overall=blocked; fi
  printf '{"status":"%s","failures":%d,"warnings":%d,"checks":[' "$overall" "$failures" "$warnings"
  for ((i = 0; i < ${#names[@]}; i++)); do
    test "$i" -eq 0 || printf ','
    printf '{"name":"%s","status":"%s","message":"%s","fix":"%s"}' \
      "$(json_escape "${names[$i]}")" \
      "$(json_escape "${statuses[$i]}")" \
      "$(json_escape "${messages[$i]}")" \
      "$(json_escape "${fixes[$i]}")"
  done
  printf ']}\n'
else
  printf 'Clotho doctor (%s)\n' "$ROOT"
  for ((i = 0; i < ${#names[@]}; i++)); do
    printf '  %-5s %-20s %s\n' "${statuses[$i]}" "${names[$i]}" "${messages[$i]}"
    if test -n "${fixes[$i]}"; then
      printf '        fix: %s\n' "${fixes[$i]}"
    fi
  done
  printf 'Result: %d failure(s), %d warning(s)\n' "$failures" "$warnings"
fi

test "$failures" -eq 0

