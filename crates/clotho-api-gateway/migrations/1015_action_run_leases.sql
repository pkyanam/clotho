alter table action_runs
  add column if not exists locked_by text,
  add column if not exists lease_expires_at timestamptz,
  add column if not exists attempt integer not null default 0;

create index if not exists action_runs_claimable_idx
  on action_runs(lease_expires_at, created_at_millis)
  where status in ('queued', 'running');
