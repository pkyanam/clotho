import { Badge } from "@cloudflare/kumo";

import { api, timeAgo } from "src/lib/api";
import { PresencePanel } from "src/components/presence-panel";
import { RepoNav } from "src/components/repo-nav";

export const dynamic = "force-dynamic";

export default async function AgentsPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const sessions = await api()
    .agentSessions(name, { limit: 50, withinSecs: 86400 })
    .catch(() => []);

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="agents" />
      <div className="mt-6">
        <h1 className="text-2xl leading-tight">agents</h1>
        <p className="mt-2 text-sm text-kumo-subtle">
          audited mcp activity, scoped token sessions, and recent provenance.
        </p>
      </div>
      <div className="mt-8 grid gap-8 lg:grid-cols-[minmax(0,1fr)_340px]">
        <ul className="divide-y divide-kumo-hairline border border-kumo-hairline">
          {sessions.length === 0 ? (
            <li className="px-4 py-8 text-sm text-kumo-inactive">
              no agent activity in the current window.
            </li>
          ) : (
            sessions.map((session) => (
              <li key={session.token_id} className="px-4 py-3">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-sm">{session.agent}</span>
                  <Badge variant="outline">{session.last_status}</Badge>
                  <Badge variant="outline">{session.tool_calls} calls</Badge>
                </div>
                <p className="mt-1 text-xs text-kumo-inactive">
                  {session.last_tool} · {timeAgo(Date.parse(session.last_seen))}
                </p>
              </li>
            ))
          )}
        </ul>
        <PresencePanel repo={name} />
      </div>
    </div>
  );
}
