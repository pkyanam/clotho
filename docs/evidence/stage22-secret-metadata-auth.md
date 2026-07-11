# Stage 22 secret-metadata authorization

**Date:** July 11, 2026

Secret values remain write-only. Organization list/detail reads require an
administrator of the owning organization; repository list/detail reads require
repository or owning-organization administration. Authorization completes
before secret-name lookup, so existing and absent names produce the same
permission response for an unauthorized caller. Invalid or malformed supplied
credentials return 401 even in open-local mode and never inherit the bootstrap
identity.

Agents may use the metadata-only repository list only when the original bearer
has the exact repository plus `list_secrets` tool scope. Organization secret
listing is denied to agents. Responses contain neither plaintext values nor
ciphertext, and the internal authorization response never returns bearer or
tool-scope material.

`tests/secret_metadata_auth.rs` uses two users, two organizations, org/repo
administrator and non-administrator grants, existing/absent names, malformed
and invalid credentials, and response-key inspection. It passed both the full
gateway test run and live `just test-collab` with fail-on-skip enabled. Gateway
clippy with warnings denied, formatting, OpenAPI drift, and the JavaScript
contract gate also passed. Fixtures were removed after the run.

This evidence does not cover membership visibility in the user/organization
directory or human provider-configuration metadata; those remain explicit
Stage 22 blockers.
