import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";

import { api } from "src/lib/api";
import {
  EmptyState,
  PageFrame,
  PageTitle,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function ReposPage() {
  const repos = await api()
    .listRepos()
    .catch(() => null);

  return (
    <PageFrame>
      <PageTitle
        title="repositories"
        description="all repositories across your organizations."
        actions={
          <Link href="/repos/new">
            <Button type="button">new repository</Button>
          </Link>
        }
      />

      {repos === null ? (
        <div className="mt-10">
          <EmptyState
            title="could not load repositories"
            description="the api gateway may be offline. start the stack and refresh."
          />
        </div>
      ) : repos.length === 0 ? (
        <div className="mt-10">
          <EmptyState
            title="no repositories yet"
            description="create your first repository to open the workspace for code, pull requests, actions, and agents."
            action={
              <Link href="/repos/new">
                <Button type="button">create repository</Button>
              </Link>
            }
          />
        </div>
      ) : (
        <ul className="mt-8 divide-y divide-kumo-hairline border border-kumo-hairline">
          {repos.map((repo) => (
            <li key={repo.name}>
              <Link
                href={`/repos/${repo.name}`}
                className="flex flex-wrap items-center justify-between gap-3 px-4 py-4 transition-colors hover:bg-kumo-elevated"
              >
                <span className="min-w-0">
                  <span className="flex flex-wrap items-center gap-2">
                    <span className="text-[0.9375rem]">{repo.name}</span>
                    <Badge variant="outline">{repo.visibility}</Badge>
                  </span>
                  {repo.description && (
                    <span className="mt-1 block text-[0.8125rem] text-kumo-inactive">
                      {repo.description}
                    </span>
                  )}
                </span>
                <span className="text-[0.8125rem] text-kumo-inactive">
                  {repo.owner} · {repo.default_branch}
                  {repo.open_pr_counter > 0
                    ? ` · ${repo.open_pr_counter} prs`
                    : ""}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </PageFrame>
  );
}
