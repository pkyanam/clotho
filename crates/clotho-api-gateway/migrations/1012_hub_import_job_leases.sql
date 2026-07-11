alter table hub_import_jobs
  add column if not exists locked_by text,
  add column if not exists lease_expires_at timestamptz;

create index if not exists hub_import_jobs_claimable_idx
  on hub_import_jobs(lease_expires_at, created_at)
  where status in ('queued', 'running');
