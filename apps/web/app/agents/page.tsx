import Link from "next/link";

import { api } from "src/lib/api";
import {
  EmptyState,
  PageFrame,
  PageTitle,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function AgentsPage() {
  const repos = await api()
    .listRepos()
    .catch(() => []);

  // Aggregate recent sessions across a sample of repos for the global view.
  const sessionsByRepo = await Promise.all(
    (repos ?? []).slice(0, 12).map(async (repo) => {
      const sessions = await api()
        .agentSessions(repo.name, { limit: 8, withinSecs: 86400 * 7 })
        .catch(() => []);
      return { repo: repo.name, sessions };
    }),
  );
  const rows = sessionsByRepo.flatMap(({ repo, sessions }) =>
    sessions.map((s) => ({ repo, ...s })),
  );

  return (
    <PageFrame>
      <PageTitle
        title="agents"
        description="agent identities and recent sessions across repositories."
      />

      {rows.length === 0 ? (
        <div className="mt-10">
          <EmptyState
            title="no agent sessions yet"
            description="when agents use the mcp tools or api against a repository, their presence appears here and on the repo workspace."
            action={
              <Link
                href="/repos"
                className="border border-kumo-hairline px-4 py-2 text-[0.875rem] hover:bg-kumo-elevated"
              >
                browse repositories
              </Link>
            }
          />
        </div>
      ) : (
        <ul className="mt-8 divide-y divide-kumo-hairline border border-kumo-hairline">
          {rows.map((row) => (
            <li
              key={`${row.repo}-${row.agent_id}-${row.token_id}-${row.last_seen}`}
              className="flex flex-wrap items-center justify-between gap-3 px-4 py-3.5"
            >
              <div>
                <Link
                  href={`/repos/${row.repo}/agents`}
                  className="text-[0.9375rem] hover:underline"
                >
                  {row.agent}
                </Link>
                <p className="mt-1 text-[0.8125rem] text-kumo-inactive">
                  {row.repo} · {row.last_tool} · {row.last_status}
                </p>
              </div>
              <span className="text-[0.8125rem] text-kumo-inactive">
                {row.tool_calls} calls
              </span>
            </li>
          ))}
        </ul>
      )}
    </PageFrame>
  );
}
