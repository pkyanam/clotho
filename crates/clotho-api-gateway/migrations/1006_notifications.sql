create table if not exists notifications (
  id bigserial primary key,
  user_id text not null references users(id) on delete cascade,
  repo_name text,
  kind text not null,
  title text not null,
  body text not null default '',
  href text not null default '',
  read_at timestamptz,
  created_at timestamptz not null default now()
);

create index if not exists notifications_user_unread_idx
  on notifications (user_id, created_at desc) where read_at is null;
