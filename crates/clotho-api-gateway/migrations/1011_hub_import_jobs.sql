create table if not exists hub_import_jobs (
  id text primary key,
  repo_id text not null references repos(id) on delete cascade,
  provider text not null default 'huggingface',
  source_repo_id text not null,
  source_revision text not null,
  request jsonb not null,
  status text not null default 'queued'
    check (status in ('queued', 'running', 'succeeded', 'failed', 'interrupted')),
  files_total bigint not null default 0,
  files_imported bigint not null default 0,
  logical_bytes bigint not null default 0,
  bytes_imported bigint not null default 0,
  arachne_files bigint not null default 0,
  security_counts jsonb not null default '{}'::jsonb,
  commit_id text not null default '',
  operation_id text not null default '',
  error text not null default '',
  created_by text not null references users(id),
  created_at timestamptz not null default now(),
  started_at timestamptz,
  completed_at timestamptz
);

create index if not exists hub_import_jobs_repo_created_idx
  on hub_import_jobs(repo_id, created_at desc);
create index if not exists hub_import_jobs_active_idx
  on hub_import_jobs(status) where status in ('queued', 'running');
