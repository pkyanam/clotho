-- Clotho secrets store (Stage 13, docs/adr/0014).
-- Values are AES-256-GCM sealed with CLOTHO_SECRETS_MASTER_KEY.
-- API responses never return plaintext — only metadata + optional last4 mask.

create table if not exists secrets (
    id text primary key,
    scope text not null check (scope in ('org', 'repo')),
    org_id text references orgs (id) on delete cascade,
    repo_id text references repos (id) on delete cascade,
    name text not null,
    description text not null default '',
    ciphertext bytea not null,
    value_last4 text not null default '',
    created_by text not null references users (id),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint secrets_scope_owner check (
        (scope = 'org' and org_id is not null and repo_id is null)
        or (scope = 'repo' and repo_id is not null)
    )
);

-- Unique name within org scope.
create unique index if not exists secrets_org_name_uidx
    on secrets (org_id, name)
    where scope = 'org';

-- Unique name within repo scope.
create unique index if not exists secrets_repo_name_uidx
    on secrets (repo_id, name)
    where scope = 'repo';

create index if not exists secrets_org_updated_idx
    on secrets (org_id, updated_at desc)
    where org_id is not null;

create index if not exists secrets_repo_updated_idx
    on secrets (repo_id, updated_at desc)
    where repo_id is not null;
