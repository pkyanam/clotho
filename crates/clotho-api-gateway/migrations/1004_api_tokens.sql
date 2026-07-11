create table if not exists api_tokens (
  id text primary key,
  user_id text not null references users(id) on delete cascade,
  name text not null default '',
  token_hash text not null unique,
  token_prefix text not null,
  scopes text[] not null default '{*}',
  created_at timestamptz not null default now(),
  last_used_at timestamptz,
  revoked_at timestamptz,
  expires_at timestamptz
);
create index if not exists api_tokens_user_idx on api_tokens (user_id) where revoked_at is null;
