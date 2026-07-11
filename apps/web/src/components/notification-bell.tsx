"use client";

import Link from "next/link";
import { useEffect, useState } from "react";

const POLL_INTERVAL_MS = 30_000;

export function NotificationBell() {
  const [unread, setUnread] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;

    const poll = async () => {
      try {
        const res = await fetch(
          "/api/notifications?unread=true&limit=1",
          { cache: "no-store" },
        );
        if (!res.ok) throw new Error("notifications unavailable");
        const result = (await res.json()) as { unread_count: number };
        if (!cancelled) setUnread(result.unread_count);
      } catch {
        if (!cancelled) setUnread(null);
      }
    };

    poll();
    const timer = setInterval(poll, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  return (
    <Link
      href="/notifications"
      className="relative flex h-9 w-9 items-center justify-center border border-kumo-hairline text-[0.8125rem] text-kumo-inactive transition-colors hover:border-kumo-contrast hover:text-kumo-default"
      aria-label={
        unread != null && unread > 0
          ? `${unread} unread notifications`
          : "notifications"
      }
    >
      🔔
      {unread != null && unread > 0 && (
        <span className="absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center bg-kumo-contrast px-1 text-[0.625rem] text-kumo-canvas">
          {unread > 99 ? "99+" : unread}
        </span>
      )}
    </Link>
  );
}
