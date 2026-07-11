#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
destination="${1:-$root/.clotho-backups/$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$destination"

compose=(docker compose -f "$root/docker-compose.dev.yml")
"${compose[@]}" exec -T postgres pg_dump --clean --if-exists -U clotho clotho >"$destination/postgres-clotho.sql"
"${compose[@]}" exec -T postgres pg_dump --clean --if-exists -U clotho forgejo >"$destination/postgres-forgejo.sql"

for volume in minio-data forgejo-data git-data vcs-data storage-data secrets-data; do
  docker run --rm \
    -v "clotho-dev_${volume}:/source:ro" \
    -v "$destination:/backup" \
    alpine:3.22 tar -czf "/backup/${volume}.tgz" -C /source .
done

secret_count="$("${compose[@]}" exec -T postgres psql -U clotho -d clotho -Atc "select count(*) from secrets")"
if docker run --rm -v clotho-dev_secrets-data:/source:ro alpine:3.22 test -s /source/master.key; then
  printf 'required\n' >"$destination/secrets-key-state"
elif [[ "$secret_count" != "0" ]]; then
  printf 'backup refused: encrypted secrets exist but the master key is externally managed; supply and escrow that key separately\n' >&2
  exit 1
else
  printf 'not-required-no-encrypted-secrets\n' >"$destination/secrets-key-state"
fi

(
  cd "$destination"
  shasum -a 256 postgres-*.sql secrets-key-state ./*.tgz >SHA256SUMS
)
printf '%s\n' "$destination"
