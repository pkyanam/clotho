import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, shortId } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import {
  EmptyState,
  PageFrame,
  SectionHeader,
  StatCell,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function BranchesPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const client = await api();
  const [detail, branches] = await Promise.all([
    client.getRepo(name).catch((e) => {
      if (e instanceof ClothoApiError && e.status === 404) notFound();
      throw e;
    }),
    client.branches(name).catch(() => []),
  ]);

  const defaultBranch = detail.default_branch;
  const sorted = [...branches].sort((a, b) => {
    if (a.name === defaultBranch) return -1;
    if (b.name === defaultBranch) return 1;
    return a.name.localeCompare(b.name);
  });
  const protectedCount = branches.filter((b) => b.protected).length;

  return (
    <PageFrame>
      <RepoNav name={name} active="branches" />

      <div className="mt-6 flex flex-wrap items-start justify-between gap-4 border-b border-kumo-hairline pb-6">
        <div className="min-w-0">
          <h1
            className="leading-tight text-kumo-default"
            style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
          >
            branches
          </h1>
          <p className="mt-2 text-[0.9375rem] leading-relaxed text-kumo-inactive">
            named lines of work on this repository. anonymous agent heads show
            on the{" "}
            <Link
              href={`/repos/${name}`}
              className="underline hover:text-kumo-default"
            >
              code
            </Link>{" "}
            overview.
          </p>
        </div>
        <div className="grid grid-cols-2 gap-2">
          <StatCell label="branches" value={branches.length} />
          <StatCell
            label="protected"
            value={protectedCount}
            muted={protectedCount === 0}
          />
        </div>
      </div>

      <div className="mt-8">
        <SectionHeader
          title="all branches"
          meta={`default ${defaultBranch}`}
        />
        {sorted.length === 0 ? (
          <div className="mt-4">
            <EmptyState
              title="no branches yet"
              description="the default branch appears after the first commit lands. push with the clotho cli or let an agent open a session."
            />
          </div>
        ) : (
          <ul className="mt-4 divide-y divide-kumo-hairline border border-kumo-hairline">
            {sorted.map((branch) => (
              <li key={branch.name}>
                <Link
                  href={
                    branch.commit.id
                      ? `/repos/${name}/commits?from=${encodeURIComponent(branch.commit.id)}`
                      : `/repos/${name}`
                  }
                  className="flex flex-wrap items-baseline justify-between gap-3 px-4 py-3 transition-colors hover:bg-kumo-elevated"
                >
                  <span className="flex min-w-0 flex-wrap items-center gap-2">
                    <span className="text-[0.9375rem] text-kumo-default">
                      {branch.name}
                    </span>
                    {branch.name === defaultBranch && (
                      <Badge variant="outline">default</Badge>
                    )}
                    {branch.protected && (
                      <Badge variant="outline">protected</Badge>
                    )}
                  </span>
                  <span className="flex min-w-0 shrink-0 flex-wrap items-baseline gap-3 text-[0.8125rem] text-kumo-inactive">
                    {branch.commit.message && (
                      <span className="max-w-[28rem] truncate">
                        {branch.commit.message.split("\n")[0]}
                      </span>
                    )}
                    {branch.commit.id && (
                      <code className="shrink-0">{shortId(branch.commit.id)}</code>
                    )}
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        )}
      </div>
    </PageFrame>
  );
}
