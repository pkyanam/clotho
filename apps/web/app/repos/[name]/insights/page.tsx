import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, shortId, timeAgo } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import {
  PageFrame,
  Panel,
  SectionHeader,
  StatCell,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function InsightsPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const client = await api();
  const [commits, opLog, pulls, issues, sessions] = await Promise.all([
    client.commits(name, { limit: 50 }).catch((e) => {
      if (e instanceof ClothoApiError && e.status === 404) notFound();
      throw e;
    }),
    client.opLog(name, 50).catch(() => []),
    client.pulls(name, "all").catch(() => []),
    client.issues(name, "all").catch(() => []),
    client.agentSessions(name, { limit: 50, withinSecs: 86400 * 7 }).catch(() => []),
  ]);

  const openPulls = pulls.filter((p) => p.state === "open").length;
  const mergedPulls = pulls.filter((p) => p.merged).length;
  const openIssues = issues.filter((i) => i.state === "open").length;
  const authors = authorBreakdown(commits);
  const agentCalls = sessions.reduce((t, s) => t + s.tool_calls, 0);

  return (
    <PageFrame>
      <RepoNav name={name} active="insights" />

      <div className="mt-6 border-b border-kumo-hairline pb-6">
        <h1
          className="leading-tight text-kumo-default"
          style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
        >
          insights
        </h1>
        <p className="mt-2 max-w-3xl text-[0.9375rem] leading-relaxed text-kumo-inactive">
          lightweight repository health — recent history, collaboration flow,
          and agent activity. windows cover the most recent 50 commits and
          operations.
        </p>
      </div>

      <div className="mt-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <StatCell label="recent commits" value={commits.length} />
        <StatCell
          label="pull requests"
          value={`${openPulls} open · ${mergedPulls} merged`}
        />
        <StatCell
          label="issues"
          value={`${openIssues} open · ${issues.length - openIssues} closed`}
        />
        <StatCell
          label="agent calls · 7d"
          value={agentCalls}
          muted={agentCalls === 0}
        />
      </div>

      <div className="mt-8 grid gap-8 lg:grid-cols-[minmax(0,1fr)_320px]">
        <section className="min-w-0">
          <SectionHeader
            title="recent operations"
            meta={`${opLog.length} in window`}
          />
          {opLog.length === 0 ? (
            <p className="mt-4 border border-kumo-hairline bg-kumo-base px-4 py-6 text-[0.875rem] text-kumo-inactive">
              no operations recorded yet.
            </p>
          ) : (
            <ul className="mt-4 divide-y divide-kumo-hairline border border-kumo-hairline">
              {opLog.slice(0, 20).map((op) => (
                <li
                  key={op.operation_id}
                  className="flex items-baseline justify-between gap-4 px-4 py-2.5"
                >
                  <span className="min-w-0 truncate text-[0.875rem] text-kumo-default">
                    {op.description}
                  </span>
                  <span className="shrink-0 text-[0.8125rem] text-kumo-inactive">
                    {shortId(op.operation_id)} · {timeAgo(op.end_time_millis)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>

        <aside className="space-y-5">
          <Panel className="p-4">
            <h2 className="text-[0.9375rem] font-medium text-kumo-default">
              authors
              <span className="ml-2 text-[0.8125rem] font-normal text-kumo-inactive">
                last {commits.length} commits
              </span>
            </h2>
            {authors.length === 0 ? (
              <p className="mt-3 text-[0.8125rem] text-kumo-inactive">
                no commits yet.
              </p>
            ) : (
              <ul className="mt-4 space-y-2.5">
                {authors.map((a) => (
                  <li
                    key={a.name}
                    className="flex items-baseline justify-between gap-3 text-[0.8125rem]"
                  >
                    <span className="min-w-0 truncate text-kumo-default">
                      {a.name}
                    </span>
                    <span className="shrink-0 text-kumo-inactive">
                      {a.count} {a.count === 1 ? "commit" : "commits"}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </Panel>

          <Panel className="p-4">
            <h2 className="text-[0.9375rem] font-medium text-kumo-default">
              collaboration
            </h2>
            <div className="mt-4 space-y-2">
              <InsightLink
                href={`/repos/${name}/pulls`}
                label="open pull requests"
                value={openPulls}
              />
              <InsightLink
                href={`/repos/${name}/issues`}
                label="open issues"
                value={openIssues}
              />
              <InsightLink
                href={`/repos/${name}/agents`}
                label="agent sessions · 7d"
                value={sessions.length}
              />
            </div>
          </Panel>
        </aside>
      </div>
    </PageFrame>
  );
}

function InsightLink({
  href,
  label,
  value,
}: {
  href: string;
  label: string;
  value: number;
}) {
  return (
    <Link
      href={href}
      className="flex items-baseline justify-between gap-3 border border-kumo-hairline px-3 py-2 text-[0.8125rem] transition-colors hover:bg-kumo-elevated"
    >
      <span className="text-kumo-inactive">{label}</span>
      <span className="text-kumo-default">{value}</span>
    </Link>
  );
}

function authorBreakdown(
  commits: Array<{ author_name: string }>,
): Array<{ name: string; count: number }> {
  const map = new Map<string, number>();
  for (const commit of commits) {
    const name = commit.author_name || "unknown";
    map.set(name, (map.get(name) ?? 0) + 1);
  }
  return [...map.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => b.count - a.count)
    .slice(0, 8);
}
