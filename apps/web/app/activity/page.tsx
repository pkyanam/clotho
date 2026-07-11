import Link from "next/link";
import type { ActivityEvent } from "@clotho/sdk-js";

import { api, timeAgo } from "src/lib/api";
import { formatEvent, isoToMillis } from "src/lib/events";
import {
  EmptyState,
  PageFrame,
  PageTitle,
  SectionHeader,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function ActivityPage() {
  const events = await (await api())
    .activity({ limit: 100 })
    .catch(() => [] as ActivityEvent[]);

  const byDay = groupByDay(events);

  return (
    <PageFrame>
      <PageTitle
        title="activity"
        description="cross-repository events from the control plane — repositories, organizations, secrets, and providers."
      />

      {events.length === 0 ? (
        <div className="mt-10">
          <EmptyState
            title="no activity yet"
            description="repository creates, secret changes, and provider connections show up here as they happen."
          />
        </div>
      ) : (
        <div className="mt-8 space-y-8">
          {byDay.map(({ day, items }) => (
            <section key={day}>
              <SectionHeader title={day} meta={`${items.length} events`} />
              <ul className="mt-3 divide-y divide-kumo-hairline border border-kumo-hairline">
                {items.map((ev) => {
                  const formatted = formatEvent(ev);
                  const row = (
                    <>
                      <span className="min-w-0">
                        <span className="block truncate text-[0.9375rem] text-kumo-default">
                          {formatted.text}
                        </span>
                        <span className="mt-0.5 block text-[0.8125rem] text-kumo-inactive">
                          {ev.event_type}
                        </span>
                      </span>
                      <span className="shrink-0 text-[0.8125rem] text-kumo-inactive">
                        {timeAgo(isoToMillis(ev.created_at))}
                      </span>
                    </>
                  );
                  return (
                    <li key={ev.id}>
                      {formatted.href ? (
                        <Link
                          href={formatted.href}
                          className="flex items-baseline justify-between gap-3 px-4 py-3 transition-colors hover:bg-kumo-elevated"
                        >
                          {row}
                        </Link>
                      ) : (
                        <div className="flex items-baseline justify-between gap-3 px-4 py-3">
                          {row}
                        </div>
                      )}
                    </li>
                  );
                })}
              </ul>
            </section>
          ))}
        </div>
      )}
    </PageFrame>
  );
}

function groupByDay(
  events: ActivityEvent[],
): Array<{ day: string; items: ActivityEvent[] }> {
  const groups: Array<{ day: string; items: ActivityEvent[] }> = [];
  for (const ev of events) {
    const day = new Date(isoToMillis(ev.created_at)).toLocaleDateString(
      "en-US",
      { year: "numeric", month: "short", day: "numeric" },
    );
    const last = groups[groups.length - 1];
    if (last && last.day === day) last.items.push(ev);
    else groups.push({ day, items: [ev] });
  }
  return groups;
}
