-- Repository semantics belong to Clotho, not the Forgejo collaboration shell.
-- Model and dataset repositories default to routing smaller artifacts through
-- Arachne while code repositories preserve the Git-LFS-compatible 10 MiB cutover.
alter table repos
    add column if not exists kind text not null default 'code',
    add column if not exists large_file_threshold_bytes bigint not null default 10485760;

alter table repos drop constraint if exists repos_kind_check;
alter table repos add constraint repos_kind_check
    check (kind in ('code', 'model', 'dataset'));

alter table repos drop constraint if exists repos_large_file_threshold_check;
alter table repos add constraint repos_large_file_threshold_check
    check (large_file_threshold_bytes >= 0);

create index if not exists repos_kind_updated_idx
    on repos (kind, updated_at desc);
