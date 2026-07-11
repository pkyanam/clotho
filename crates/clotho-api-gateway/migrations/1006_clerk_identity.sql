-- Stage 17 / ADR-0018: map Clerk human identities into Clotho users/orgs.
-- Agents never appear here — they stay on agents / agent_tokens (ADR-0005).

create table if not exists clerk_user_links (
    clerk_user_id text primary key,
    user_id text not null references users (id) on delete cascade,
    created_at timestamptz not null default now()
);

create unique index if not exists clerk_user_links_user_id_uidx
    on clerk_user_links (user_id);

create table if not exists clerk_org_links (
    clerk_org_id text primary key,
    org_id text not null references orgs (id) on delete cascade,
    created_at timestamptz not null default now()
);

create unique index if not exists clerk_org_links_org_id_uidx
    on clerk_org_links (org_id);
