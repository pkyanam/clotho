-- GPU intent is repository policy, not a provider-specific workflow trick.
alter table actions_configs
    add column if not exists accelerator text not null default 'cpu',
    add column if not exists gpu_types text[] not null default '{}';

alter table actions_configs drop constraint if exists actions_configs_accelerator_check;
alter table actions_configs add constraint actions_configs_accelerator_check
    check (accelerator in ('cpu', 'gpu'));
