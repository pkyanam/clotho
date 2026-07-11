#!/usr/bin/env bash
set -euo pipefail

backup="${1:?usage: restore-drill.sh BACKUP_DIRECTORY}"
backup="$(cd "$backup" && pwd)"
test -s "$backup/postgres-clotho.sql"
test -s "$backup/postgres-forgejo.sql"
(
  cd "$backup"
  shasum -a 256 -c SHA256SUMS
)

suffix="$(date +%s)-$$"
postgres="clotho-restore-postgres-$suffix"
network="clotho-restore-$suffix"
cleanup() {
  docker rm -f "$postgres" >/dev/null 2>&1 || true
  docker network rm "$network" >/dev/null 2>&1 || true
  for volume in minio-data forgejo-data git-data vcs-data storage-data secrets-data; do
    docker volume rm "clotho-restore-$suffix-$volume" >/dev/null 2>&1 || true
  done
}
trap cleanup EXIT

docker network create "$network" >/dev/null
docker run -d --name "$postgres" --network "$network" \
  -e POSTGRES_USER=clotho -e POSTGRES_PASSWORD=restore-drill -e POSTGRES_DB=clotho \
  postgres:17-alpine >/dev/null
for _ in $(seq 1 30); do
  docker exec "$postgres" pg_isready -U clotho >/dev/null 2>&1 && break
  sleep 1
done
docker exec "$postgres" createuser -U clotho forgejo
docker exec "$postgres" createdb -U clotho -O forgejo forgejo
docker exec -i "$postgres" psql -v ON_ERROR_STOP=1 -U clotho -d clotho <"$backup/postgres-clotho.sql" >/dev/null
docker exec -i "$postgres" psql -v ON_ERROR_STOP=1 -U clotho -d forgejo <"$backup/postgres-forgejo.sql" >/dev/null
docker exec "$postgres" psql -U clotho -d clotho -Atc \
  "select count(*) from information_schema.tables where table_schema='public'" | grep -Eq '^[1-9][0-9]*$'
docker exec "$postgres" psql -U clotho -d forgejo -Atc \
  "select count(*) from information_schema.tables where table_schema='public'" | grep -Eq '^[1-9][0-9]*$'

for volume in minio-data forgejo-data git-data vcs-data storage-data secrets-data; do
  restored="clotho-restore-$suffix-$volume"
  docker volume create "$restored" >/dev/null
  docker run --rm -v "$restored:/restore" -v "$backup:/backup:ro" alpine:3.22 \
    tar -xzf "/backup/${volume}.tgz" -C /restore
done
if grep -qx required "$backup/secrets-key-state"; then
  docker run --rm -v "clotho-restore-$suffix-secrets-data:/restore:ro" alpine:3.22 \
    test -s /restore/master.key
fi

printf 'restore drill passed: Postgres plus six durable volumes\n'
