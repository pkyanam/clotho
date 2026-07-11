import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";

import { api } from "src/lib/api";
import {
  EmptyState,
  PageFrame,
  PageTitle,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function ReposPage({
  searchParams,
}: {
  searchParams: Promise<{ cursor?: string }>;
}) {
  const { cursor } = await searchParams;
  const page = await (await api())
    .listReposPage({ limit: 50, cursor })
    .catch(() => null);
  const repos = page?.repos ?? null;

  return (
    <PageFrame>
      <PageTitle
        title="repositories"
        description="all repositories across your organizations."
        actions={
          <Link href="/repos/new">
            <Button type="button" variant="primary">
              new repository
            </Button>
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
                <Button type="button" variant="primary">
                  create repository
                </Button>
              </Link>
            }
          />
        </div>
      ) : (
        <>
          <ul className="clotho-panel mt-8 divide-y divide-kumo-hairline overflow-hidden">
            {repos.map((repo) => (
              <li key={repo.name}>
                <Link
                  href={`/repos/${repo.name}`}
                  className="clotho-row flex flex-wrap items-center justify-between gap-3 px-4 py-4"
                >
                  <span className="min-w-0">
                    <span className="flex flex-wrap items-center gap-2">
                      <span
                        title={repo.name}
                        className="min-w-0 flex-1 truncate text-[0.9375rem]"
                      >
                        {repo.name}
                      </span>
                      <Badge variant="outline">{repo.kind}</Badge>
                      <Badge variant="outline">{repo.visibility}</Badge>
                    </span>
                    {repo.description && (
                      <span className="mt-1 block truncate text-[0.8125rem] text-kumo-inactive">
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
          {page?.next_cursor && (
            <div className="mt-5 flex justify-end">
              <Link
                href={`/repos?cursor=${encodeURIComponent(page.next_cursor)}`}
                className="rounded-md border border-kumo-hairline bg-kumo-base px-4 py-2 text-[0.8125rem] text-accent-strong hover:border-accent"
              >
                next repositories →
              </Link>
            </div>
          )}
        </>
      )}
    </PageFrame>
  );
}
