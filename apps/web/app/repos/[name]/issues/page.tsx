import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, timeAgo } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";

export const dynamic = "force-dynamic";

export default async function IssuesPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const issues = await api()
    .issues(name, "all")
    .catch((e) => {
      if (e instanceof ClothoApiError && e.status === 404) notFound();
      throw e;
    });

  const open = issues.filter((issue) => issue.state === "open").length;

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="issues" />

      <div className="mt-6 flex flex-wrap items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl leading-tight">issues</h1>
          <p className="mt-2 text-sm text-kumo-subtle">
            triage work, agent findings, and human follow-ups inside clotho.
          </p>
        </div>
        <Link
          href={`/repos/${name}/issues/new`}
          className="border border-kumo-hairline px-3 py-2 text-xs hover:bg-kumo-elevated"
        >
          new issue
        </Link>
      </div>

      <div className="mt-6 flex flex-wrap gap-2">
        <Badge variant="outline">{open} open</Badge>
        <Badge variant="outline">{issues.length - open} closed</Badge>
      </div>

      {issues.length === 0 ? (
        <p className="mt-8 border border-kumo-hairline px-4 py-8 text-sm text-kumo-inactive">
          no issues yet.
        </p>
      ) : (
        <ul className="mt-8 divide-y divide-kumo-hairline border border-kumo-hairline">
          {issues.map((issue) => (
            <li key={issue.number}>
              <Link
                href={`/repos/${name}/issues/${issue.number}`}
                className="flex flex-col gap-2 px-4 py-3 transition-colors hover:bg-kumo-elevated sm:flex-row sm:items-baseline sm:justify-between"
              >
                <span className="min-w-0">
                  <span className="flex flex-wrap items-center gap-2">
                    <span className="truncate text-sm">{issue.title}</span>
                    <Badge variant="outline">{issue.state}</Badge>
                    {issue.comments > 0 && (
                      <Badge variant="outline">{issue.comments} comments</Badge>
                    )}
                  </span>
                  <span className="mt-1 block text-xs text-kumo-inactive">
                    #{issue.number} opened by {issue.user.login}
                  </span>
                </span>
                <span className="shrink-0 text-xs text-kumo-inactive">
                  {timeAgo(Date.parse(issue.updated_at))}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
