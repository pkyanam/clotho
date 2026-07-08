-- Durable Clotho Actions state (ADR-0012): Actions are Clotho-owned records.
-- Forgejo commit statuses are compatibility output, not the source of truth.
--
-- This gateway migration starts at 1001 because the dev Postgres database is
-- shared with clotho-agent-gateway, whose embedded sqlx migrations already use
-- the lower version range.

create sequence if not exists clotho_action_run_seq;

create table if not exists action_runs (
    id text primary key,
    repo text not null,
    commit_id text not null,
    branch text not null,
    status text not null,
    conclusion text not null default '',
    trigger text not null,
    actor text not null,
    provider text not null,
    sandbox_id text not null default '',
    created_at_millis bigint not null,
    started_at_millis bigint not null default 0,
    finished_at_millis bigint not null default 0,
    duration_ms bigint not null default 0,
    jobs jsonb not null default '[]'::jsonb
);

create index if not exists action_runs_repo_created_idx
    on action_runs (repo, created_at_millis desc, id desc);

create table if not exists action_logs (
    run_id text primary key references action_runs (id) on delete cascade,
    log_text text not null default '',
    updated_at timestamptz not null default now()
);

create table if not exists actions_configs (
    repo text primary key,
    enabled boolean not null,
    provider text not null,
    default_image text not null,
    timeout_seconds integer not null,
    updated_at timestamptz not null default now()
);
