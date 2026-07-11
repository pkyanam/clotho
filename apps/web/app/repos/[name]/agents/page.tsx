import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";
import { api, timeAgo } from "src/lib/api";
import { PresencePanel } from "src/components/presence-panel";
import { RepoNav } from "src/components/repo-nav";
import {
  EmptyState,
  PageFrame,
  SectionHeader,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function AgentsPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const sessions = await (await api())
    .agentSessions(name, { limit: 50, withinSecs: 86400 })
    .catch(() => []);

  return (
    <PageFrame>
      <RepoNav name={name} active="agents" />

      <div className="mt-6 border-b border-kumo-hairline pb-6">
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div>
            <h1
              className="leading-tight text-kumo-default"
              style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
            >
              agents
            </h1>
            <p className="mt-2 max-w-3xl text-[0.9375rem] leading-relaxed text-kumo-inactive">
              every agent action on this repository is audited — identity, tool,
              and outcome. sessions shown from the last 24 hours.
            </p>
          </div>
          <Link href="/agents">
            <Button type="button" variant="outline">
              manage agents →
            </Button>
          </Link>
        </div>
      </div>

      <div className="mt-8 grid gap-8 lg:grid-cols-[minmax(0,1fr)_320px]">
        <section className="min-w-0">
          <SectionHeader
            title="sessions"
            meta={`${sessions.length} in the last 24h`}
          />
          {sessions.length === 0 ? (
            <div className="mt-4">
              <EmptyState
                title="no agent sessions in the last 24 hours"
                description="agents connect to clotho with scoped tokens and work through audited tools. when one touches this repository, its session appears here."
              />
            </div>
          ) : (
            <ul className="mt-4 divide-y divide-kumo-hairline border border-kumo-hairline">
              {sessions.map((session) => (
                <li
                  key={session.token_id}
                  className="flex flex-wrap items-center justify-between gap-3 px-4 py-3"
                >
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-[0.9375rem] text-kumo-default">
                        {session.agent}
                      </span>
                      <Badge variant="outline">{session.last_status}</Badge>
                    </div>
                    <p className="mt-1 text-[0.8125rem] text-kumo-inactive">
                      last tool {session.last_tool} · {session.tool_calls}{" "}
                      {session.tool_calls === 1 ? "call" : "calls"} · first seen{" "}
                      {timeAgo(Date.parse(session.first_seen))}
                    </p>
                  </div>
                  <span className="shrink-0 text-[0.8125rem] text-kumo-inactive">
                    {timeAgo(Date.parse(session.last_seen))}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>

        <aside>
          <PresencePanel repo={name} />
        </aside>
      </div>
    </PageFrame>
  );
}
