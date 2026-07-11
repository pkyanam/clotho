-- Stable keyset pagination for the global activity feed.
create index if not exists activity_events_created_id_idx
    on activity_events (created_at desc, id desc);
