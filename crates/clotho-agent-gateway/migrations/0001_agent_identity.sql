-- Agent identity (docs/prd.md §2, §6): agents are first-class non-human
-- identities in their own tables — never a flag on a human user row. Tokens
-- are scoped credentials bound to an agent; every MCP tool invocation lands
-- in the audit log with full provenance.

create table agents (
    id uuid primary key default gen_random_uuid(),
    name text not null unique
        check (name ~ '^[a-z0-9][a-z0-9_-]*$' and length(name) <= 100),
    description text not null default '',
    created_at timestamptz not null default now(),
    -- Soft kill switch: a disabled agent's tokens all stop working at once.
    disabled_at timestamptz
);

create table agent_tokens (
    id uuid primary key default gen_random_uuid(),
    agent_id uuid not null references agents (id) on delete cascade,
    -- SHA-256 of the bearer token; the plaintext is shown once at mint time
    -- and never stored.
    token_hash bytea not null unique,
    -- Scopes: which repos and which MCP tools this token may touch.
    -- '*' grants all. Empty arrays grant nothing.
    allowed_repos text[] not null,
    allowed_tools text[] not null,
    created_at timestamptz not null default now(),
    expires_at timestamptz,
    revoked_at timestamptz
);

create index agent_tokens_agent_id_idx on agent_tokens (agent_id);

create table agent_audit_log (
    id bigint generated always as identity primary key,
    agent_id uuid not null references agents (id),
    token_id uuid not null references agent_tokens (id),
    tool text not null,
    repo text not null,
    -- SHA-256 of the canonical-JSON tool arguments: provenance without
    -- retaining potentially large or sensitive payloads.
    args_digest bytea not null,
    -- 'ok' | 'denied' | 'error'
    status text not null,
    error text,
    occurred_at timestamptz not null default now()
);

create index agent_audit_log_agent_id_idx on agent_audit_log (agent_id, occurred_at desc);
