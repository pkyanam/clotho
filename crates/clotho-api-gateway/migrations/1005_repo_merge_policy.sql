-- Clotho-owned merge and branch policy per repository (Slice E).
-- Enforced at merge time on the api-gateway; not delegated to the collaboration provider.

create table if not exists repo_merge_policies (
    repo_id text primary key references repos (id) on delete cascade,
    require_passing_actions boolean not null default false,
    block_merge_when_conflicted boolean not null default true,
    require_review_approvals int not null default 0,
    protect_default_branch boolean not null default false,
    updated_at timestamptz not null default now()
);
