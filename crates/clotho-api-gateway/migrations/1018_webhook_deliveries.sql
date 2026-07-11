-- Durable replay admission for internal collaboration-provider webhooks.
-- Provider delivery ids and exact request bodies are SHA-256 hashed before
-- storage. Rows expire after 24 hours and are removed in bounded batches.
create table if not exists webhook_deliveries (
    delivery_hash text primary key,
    payload_hash text not null,
    org_id text not null references orgs (id) on delete cascade,
    repo_id text not null references repos (id) on delete cascade,
    event_type text not null,
    commit_sha text not null,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null default now() + interval '24 hours',
    constraint webhook_deliveries_delivery_hash_check check (
        length(delivery_hash) = 64 and delivery_hash ~ '^[0-9a-f]{64}$'
    ),
    constraint webhook_deliveries_payload_hash_check check (
        length(payload_hash) = 64 and payload_hash ~ '^[0-9a-f]{64}$'
    ),
    constraint webhook_deliveries_event_type_check check (
        length(event_type) between 1 and 64
    ),
    constraint webhook_deliveries_commit_sha_check check (
        length(commit_sha) between 1 and 64 and commit_sha ~ '^[0-9A-Fa-f]+$'
    ),
    constraint webhook_deliveries_expiry_check check (expires_at > created_at)
);

create index if not exists webhook_deliveries_expires_idx
    on webhook_deliveries (expires_at);

create index if not exists webhook_deliveries_repo_created_idx
    on webhook_deliveries (repo_id, created_at desc);
