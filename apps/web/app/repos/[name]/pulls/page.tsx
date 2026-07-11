import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, shortId, timeAgo } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import { FilterTab } from "src/components/ui/filter-tabs";
import { EmptyState, PageFrame } from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

type StateFilter = "open" | "closed" | "all";

function parseState(raw: string | undefined): StateFilter {
  return raw === "closed" || raw === "all" ? raw : "open";
}

export default async function PullsPage({
  params,
  searchParams,
}: {
  params: Promise<{ name: string }>;
  searchParams: Promise<{ state?: string }>;
}) {
  const { name } = await params;
  const state = parseState((await searchParams).state);
  const pulls = await (await api())
    .pulls(name, "all")
    .catch((e) => {
      if (e instanceof ClothoApiError && e.status === 404) notFound();
      throw e;
    });

  const open = pulls.filter((pull) => pull.state === "open");
  const closed = pulls.filter((pull) => pull.state !== "open");
  const visible = state === "open" ? open : state === "closed" ? closed : pulls;

  return (
    <PageFrame>
      <RepoNav name={name} active="pulls" />

      <div className="mt-6 border-b border-kumo-hairline pb-6">
        <h1
          className="leading-tight text-kumo-default"
          style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
        >
          pull requests
        </h1>
        <p className="mt-2 max-w-3xl text-[0.9375rem] leading-relaxed text-kumo-inactive">
          review, discuss, and merge changes from humans and agents. open pull
          requests from the clotho cli, sdk, or an agent session.
        </p>
      </div>

      <div className="mt-6 flex flex-wrap items-center gap-1">
        <FilterTab
          href={`/repos/${name}/pulls`}
          label={`open · ${open.length}`}
          current={state === "open"}
        />
        <FilterTab
          href={`/repos/${name}/pulls?state=closed`}
          label={`closed · ${closed.length}`}
          current={state === "closed"}
        />
        <FilterTab
          href={`/repos/${name}/pulls?state=all`}
          label={`all · ${pulls.length}`}
          current={state === "all"}
        />
      </div>

      {visible.length === 0 ? (
        <div className="mt-6">
          <EmptyState
            title={
              state === "open"
                ? "no open pull requests"
                : state === "closed"
                  ? "no closed pull requests"
                  : "no pull requests yet"
            }
            description={
              pulls.length === 0
                ? "when a human or agent submits a change for review, it lands here with a structured diff, checks, and a merge control."
                : "nothing matches this filter."
            }
          />
        </div>
      ) : (
        <ul className="mt-6 divide-y divide-kumo-hairline border border-kumo-hairline">
          {visible.map((pull) => (
            <li key={pull.number}>
              <Link
                href={`/repos/${name}/pulls/${pull.number}`}
                className="flex flex-col gap-2 px-4 py-3.5 transition-colors hover:bg-kumo-elevated sm:flex-row sm:items-baseline sm:justify-between"
              >
                <span className="min-w-0">
                  <span className="flex flex-wrap items-center gap-2">
                    <span className="truncate text-[0.9375rem] text-kumo-default">
                      {pull.title}
                    </span>
                    <Badge variant="outline">
                      {pull.merged ? "merged" : pull.state}
                    </Badge>
                    {!pull.merged && pull.state === "open" && !pull.mergeable && (
                      <Badge variant="outline">conflicts</Badge>
                    )}
                  </span>
                  <span className="mt-1 block text-[0.8125rem] text-kumo-inactive">
                    #{pull.number} · {pull.user.login} · {pull.head.ref} →{" "}
                    {pull.base.ref} · head {shortId(pull.head.sha)}
                    {pull.comments > 0
                      ? ` · ${pull.comments} ${pull.comments === 1 ? "comment" : "comments"}`
                      : ""}
                  </span>
                </span>
                <span className="shrink-0 text-[0.8125rem] text-kumo-inactive">
                  {timeAgo(Date.parse(pull.updated_at))}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </PageFrame>
  );
}
