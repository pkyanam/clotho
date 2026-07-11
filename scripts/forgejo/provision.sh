#!/bin/sh
# One-shot Forgejo provisioning for the dev stack (docker-compose.dev.yml).
# Runs in a throwaway container off the same pinned Forgejo image, after the
# forgejo service is healthy (so app.ini exists and migrations have run):
#   1. creates the `clotho` admin user (registration is disabled),
#   2. mints an API token for the Clotho API gateway and drops it in the
#      shared token volume, where clotho-api-gateway reads it
#      (CLOTHO_FORGEJO_TOKEN_FILE).
# Idempotent: safe to re-run on an already-provisioned stack.
set -eu

CONFIG=/data/gitea/conf/app.ini
TOKEN_FILE=/run/clotho/forgejo-token
ADMIN_USER=clotho
ADMIN_PASSWORD=clotho-dev # dev-only credentials

forgejo_cli() {
  su git -c "forgejo --config $CONFIG $*"
}

if ! forgejo_cli admin user list --admin | awk 'NR>1 {print $2}' | grep -qx "$ADMIN_USER"; then
  forgejo_cli admin user create --admin \
    --username "$ADMIN_USER" \
    --password "$ADMIN_PASSWORD" \
    --email "admin@clotho.internal" \
    --must-change-password=false
  echo "provision: created admin user $ADMIN_USER"
fi

if [ ! -s "$TOKEN_FILE" ]; then
  # The shell performs the redirect as root, so constrain creation before the
  # token exists and then hand ownership to the uid shared with Clotho.
  umask 077
  forgejo_cli admin user generate-access-token \
    --username "$ADMIN_USER" \
    --token-name clotho-api-gateway \
    --scopes all --raw >"$TOKEN_FILE"
  echo "provision: wrote API token to $TOKEN_FILE"
fi

# Also repair permissions on volumes provisioned by an older release.
chown git:git "$TOKEN_FILE"
chmod 0600 "$TOKEN_FILE"

echo "provision: done"
