-- Clotho control plane: users, orgs, memberships, repos, permissions, and activity.
--
-- These tables are the v2 source of truth for identity, ownership, and repo
-- metadata. Forgejo fields are kept as provider mappings only; the Clotho row
-- owns the repo record.
--
-- Stage 11 auth is a deterministic bootstrap placeholder. Real human auth and
-- encrypted secrets are explicitly deferred.

-- Users are human identities. In the Stage 11 placeholder, a deterministic
-- bootstrap user is created on first start from env/defaults.
create table if not exists users (
    id text primary key,
    name text unique not null,
    email text not null default '',
    display_name text not null default '',
    created_at timestamptz not null default now()
);

-- Orgs are Clotho organizations. Each org maps to a Forgejo owner/user for
-- the git collaboration shell, but Clotho owns the record.
create table if not exists orgs (
    id text primary key,
    name text unique not null,
    display_name text not null default '',
    forgejo_owner text not null default 'clotho',
    created_by text not null references users (id),
    created_at timestamptz not null default now()
);

-- Many-to-many: users belong to orgs with a role.
create table if not exists org_memberships (
    org_id text not null references orgs (id) on delete cascade,
    user_id text not null references users (id) on delete cascade,
    role text not null default 'member',
    primary key (org_id, user_id)
);

-- Clotho-owned repositories. The (org_id, name) pair is unique, but the web
-- URLs in this slice still route by repo name alone, so bootstrap/dev usage
-- should keep names unique across orgs for now.
create table if not exists repos (
    id text primary key,
    org_id text not null references orgs (id),
    name text not null,
    description text not null default '',
    visibility text not null default 'public',
    default_branch text not null default 'main',
    forgejo_owner text not null default 'clotho',
    forgejo_repo_id bigint,
    forgejo_full_name text,
    created_by text not null references users (id),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (org_id, name)
);

-- User-level repo permissions. In Stage 11 this is populated with a blanket
-- admin grant for the creating user; fine-grained policy is deferred.
create table if not exists repo_permissions (
    repo_id text not null references repos (id) on delete cascade,
    user_id text not null references users (id) on delete cascade,
    permission text not null default 'read',
    primary key (repo_id, user_id)
);

create index if not exists repos_org_updated_idx
    on repos (org_id, updated_at desc);

-- Activity feed: a simple audit-aware event stream. Payloads are JSON so the
-- schema stays small while Stage 11 builds dashboards/provenance.
create table if not exists activity_events (
    id bigserial primary key,
    actor_id text not null references users (id),
    org_id text references orgs (id),
    repo_id text references repos (id) on delete cascade,
    event_type text not null,
    payload jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now()
);

create index if not exists activity_events_org_created_idx
    on activity_events (org_id, created_at desc);

create index if not exists activity_events_repo_created_idx
    on activity_events (repo_id, created_at desc);
