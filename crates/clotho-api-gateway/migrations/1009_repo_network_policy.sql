-- Clotho owns private-network intent; workflow files and Forgejo do not.
alter table repos
    add column if not exists network_mode text not null default 'public',
    add column if not exists network_tags text[] not null default '{}';

alter table repos drop constraint if exists repos_network_mode_check;
alter table repos add constraint repos_network_mode_check
    check (network_mode in ('public', 'tailscale'));
