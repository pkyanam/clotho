-- Common persisted idempotency contract for retryable public mutations.
-- Keys are hashed before storage and scoped to an immutable organization plus
-- authenticated principal. Response bodies never contain secret values.
create table if not exists idempotency_records (
    org_id text not null references orgs (id) on delete cascade,
    principal_id text not null,
    key_hash text not null,
    operation text not null,
    request_fingerprint text not null,
    resource_kind text not null,
    resource_id text not null,
    response_status integer not null,
    response_body jsonb not null,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null default now() + interval '24 hours',
    primary key (org_id, principal_id, key_hash)
);

create index if not exists idempotency_records_expires_idx
    on idempotency_records (expires_at);
