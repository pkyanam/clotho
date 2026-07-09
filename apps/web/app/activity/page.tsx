import { api, timeAgo } from "src/lib/api";
import {
  EmptyState,
  PageFrame,
  PageTitle,
} from "src/components/ui/page-frame";
import type { ActivityEvent } from "@clotho/sdk-js";

export const dynamic = "force-dynamic";

function isoToMillis(iso: string): number {
  const t = Date.parse(iso);
  return Number.isFinite(t) ? t : 0;
}

export default async function ActivityPage() {
  const events = await api()
    .activity({ limit: 50 })
    .catch(() => [] as ActivityEvent[]);

  return (
    <PageFrame>
      <PageTitle
        title="activity"
        description="cross-repository events from the control plane."
      />

      {events.length === 0 ? (
        <div className="mt-10">
          <EmptyState
            title="no activity yet"
            description="repository creates, secret changes, and provider connections will show up here."
          />
        </div>
      ) : (
        <ul className="mt-8 divide-y divide-kumo-hairline border border-kumo-hairline">
          {events.map((ev) => (
            <li
              key={ev.id}
              className="flex flex-wrap items-baseline justify-between gap-3 px-4 py-3.5"
            >
              <div>
                <p className="text-[0.9375rem]">{formatEvent(ev)}</p>
                <p className="mt-1 text-[0.8125rem] text-kumo-inactive">
                  {ev.event_type}
                </p>
              </div>
              <span className="text-[0.8125rem] text-kumo-inactive">
                {timeAgo(isoToMillis(ev.created_at))}
              </span>
            </li>
          ))}
        </ul>
      )}
    </PageFrame>
  );
}

function formatEvent(ev: ActivityEvent): string {
  const payload = (ev.payload ?? {}) as Record<string, unknown>;
  switch (ev.event_type) {
    case "repo.created":
      return `repository ${String(payload.repo_name ?? "")} created`;
    case "org.created":
      return `organization ${String(payload.org_name ?? "")} created`;
    case "secret.created":
      return `secret ${String(payload.name ?? "")} created`;
    case "secret.updated":
      return `secret ${String(payload.name ?? "")} rotated`;
    case "secret.deleted":
      return `secret ${String(payload.name ?? "")} deleted`;
    case "provider.connected":
      return `provider ${String(payload.provider ?? "")} connected`;
    default:
      return ev.event_type.replace(/\./g, " ");
  }
}
