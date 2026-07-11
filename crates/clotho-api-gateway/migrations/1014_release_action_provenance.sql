alter table action_runs
  add column if not exists workflow text not null default 'ci',
  add column if not exists release_version text not null default '',
  add column if not exists release_manifest_sha256 text not null default '';

create index if not exists action_runs_release_idx
  on action_runs(repo, release_version, created_at_millis desc)
  where release_version <> '';
