import Link from "next/link";
import { ClothoApiError, type Notification } from "@clotho/sdk-js";

import { api, timeAgo } from "src/lib/api";
import { MarkAllReadButton } from "src/components/mark-all-read-button";
import { EmptyState, PageFrame } from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function NotificationsPage() {
  let notifications: Notification[] = [];
  let unread_count = 0;
  let unavailable = false;

  try {
    const result = await (await api()).notifications({ limit: 50 });
    notifications = result.notifications;
    unread_count = result.unread_count;
  } catch (error) {
    if (error instanceof ClothoApiError && error.status === 404) {
      unavailable = true;
    } else {
      throw error;
    }
  }

  return (
    <PageFrame>
      <div className="flex flex-wrap items-start justify-between gap-4 border-b border-kumo-hairline pb-6">
        <div>
          <h1
            className="leading-tight text-kumo-default"
            style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
          >
            notifications
          </h1>
          <p className="mt-2 text-[0.9375rem] text-kumo-inactive">
            assignments, comments, mentions, and action failures — polled, not
            pushed.
          </p>
        </div>
        {unread_count > 0 && <MarkAllReadButton />}
      </div>

      {unavailable ? (
        <div className="mt-6">
          <EmptyState
            title="notifications unavailable"
            description="the api gateway is running an older build without the notifications route. rebuild clotho-api-gateway (docker compose up --build clotho-api-gateway) and refresh."
          />
        </div>
      ) : notifications.length === 0 ? (
        <div className="mt-6">
          <EmptyState
            title="nothing here yet"
            description="notifications appear when you are assigned to an issue, someone comments, you are @mentioned, or an action run fails."
          />
        </div>
      ) : (
        <ul className="mt-6 divide-y divide-kumo-hairline border border-kumo-hairline">
          {notifications.map((n) => (
            <li key={n.id}>
              <Link
                href={n.href || "/"}
                className={`flex flex-col gap-1 px-4 py-3.5 transition-colors hover:bg-kumo-elevated sm:flex-row sm:items-baseline sm:justify-between ${
                  n.read_at ? "opacity-70" : ""
                }`}
              >
                <span className="min-w-0">
                  <span className="text-[0.75rem] uppercase tracking-wide text-kumo-inactive">
                    {n.kind.replace(/_/g, " ")}
                    {n.repo_name ? ` · ${n.repo_name}` : ""}
                  </span>
                  <span className="mt-0.5 block text-[0.9375rem] text-kumo-default">
                    {n.title}
                  </span>
                  {n.body ? (
                    <span className="mt-1 block truncate text-[0.8125rem] text-kumo-inactive">
                      {n.body}
                    </span>
                  ) : null}
                </span>
                <span className="shrink-0 text-[0.8125rem] text-kumo-inactive">
                  {timeAgo(Date.parse(n.created_at))}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </PageFrame>
  );
}
