create table if not exists repo_releases (
  id text primary key,
  repo_id text not null references repos(id) on delete cascade,
  version text not null,
  commit_id text not null,
  manifest jsonb not null,
  manifest_sha256 text not null,
  created_by text not null references users(id),
  created_at timestamptz not null default now(),
  unique (repo_id, version)
);

create index if not exists repo_releases_repo_created_idx
  on repo_releases(repo_id, created_at desc);
