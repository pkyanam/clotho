import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError, type Commit } from "@clotho/sdk-js";

import { api, shortId, timeAgo } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import {
  EmptyState,
  PageFrame,
  SectionHeader,
  StatCell,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

const PAGE_SIZE = 50;

export default async function CommitsPage({
  params,
  searchParams,
}: {
  params: Promise<{ name: string }>;
  searchParams: Promise<{ from?: string }>;
}) {
  const { name } = await params;
  const { from } = await searchParams;
  const client = await api();
  const [detail, commits] = await Promise.all([
    client.getRepo(name).catch((e) => {
      if (e instanceof ClothoApiError && e.status === 404) notFound();
      throw e;
    }),
    client
      .commits(name, { fromCommitId: from, limit: PAGE_SIZE })
      .catch(() => [] as Commit[]),
  ]);

  const authors = new Set(commits.map((c) => c.author_email || c.author_name));
  const oldest = commits[commits.length - 1];
  const nextFrom =
    commits.length === PAGE_SIZE ? oldest?.parent_commit_ids[0] : undefined;
  const byDay = groupByDay(commits);

  return (
    <PageFrame>
      <RepoNav name={name} active="commits" />

      <div className="mt-6 flex flex-wrap items-start justify-between gap-4 border-b border-kumo-hairline pb-6">
        <div className="min-w-0">
          <h1
            className="leading-tight text-kumo-default"
            style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
          >
            commits
          </h1>
          <p className="mt-2 text-[0.9375rem] leading-relaxed text-kumo-inactive">
            history {from ? `from ${shortId(from)}` : `of ${detail.default_branch}`}.
            change ids stay stable across rewrites — use them when talking to
            agents.
          </p>
        </div>
        <div className="grid grid-cols-2 gap-2">
          <StatCell label="showing" value={commits.length} />
          <StatCell label="authors" value={authors.size} />
        </div>
      </div>

      {commits.length === 0 ? (
        <div className="mt-8">
          <EmptyState
            title="no commits yet"
            description="push your first commit with the clotho cli or sdk, or let an agent open a session and write the first change."
          />
        </div>
      ) : (
        <div className="mt-8 space-y-8">
          {byDay.map(({ day, items }) => (
            <section key={day}>
              <SectionHeader title={day} meta={`${items.length} commits`} />
              <ul className="mt-3 divide-y divide-kumo-hairline border border-kumo-hairline">
                {items.map((commit) => (
                  <li
                    key={commit.commit_id}
                    className="flex flex-wrap items-baseline justify-between gap-3 px-4 py-3"
                  >
                    <span className="min-w-0">
                      <span className="block truncate text-[0.9375rem] text-kumo-default">
                        {commit.description.split("\n")[0] || "(no description)"}
                      </span>
                      <span className="mt-1 flex flex-wrap items-center gap-2 text-[0.8125rem] text-kumo-inactive">
                        <span>{commit.author_name || "unknown"}</span>
                        <span>·</span>
                        <span>change {shortId(commit.change_id)}</span>
                        {commit.parent_commit_ids.length > 1 && (
                          <Badge variant="outline">merge</Badge>
                        )}
                      </span>
                    </span>
                    <span className="flex shrink-0 items-baseline gap-3 text-[0.8125rem] text-kumo-inactive">
                      <code>{shortId(commit.commit_id)}</code>
                      <span>{timeAgo(commit.timestamp_millis)}</span>
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          ))}

          {nextFrom && (
            <div className="flex justify-center">
              <Link
                href={`/repos/${name}/commits?from=${encodeURIComponent(nextFrom)}`}
                className="border border-kumo-hairline px-4 py-2 text-[0.875rem] text-kumo-inactive transition-colors hover:bg-kumo-elevated hover:text-kumo-default"
              >
                older commits
              </Link>
            </div>
          )}
        </div>
      )}
    </PageFrame>
  );
}

function groupByDay(commits: Commit[]): Array<{ day: string; items: Commit[] }> {
  const groups: Array<{ day: string; items: Commit[] }> = [];
  for (const commit of commits) {
    const day = new Date(commit.timestamp_millis).toLocaleDateString("en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
    const last = groups[groups.length - 1];
    if (last && last.day === day) last.items.push(commit);
    else groups.push({ day, items: [commit] });
  }
  return groups;
}
