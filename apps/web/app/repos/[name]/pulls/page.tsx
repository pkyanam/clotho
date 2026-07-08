import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, shortId, timeAgo } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";

export const dynamic = "force-dynamic";

export default async function PullsPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const pulls = await api()
    .pulls(name, "all")
    .catch((e) => {
      if (e instanceof ClothoApiError && e.status === 404) notFound();
      throw e;
    });

  return (
    <div className="mx-auto max-w-6xl px-6 py-16">
      <RepoNav name={name} active="pulls" />

      <div className="mt-8 flex flex-wrap items-center gap-4">
        <h1
          className="leading-[1.2]"
          style={{ fontSize: "clamp(1.5rem, 3vw, 1.875rem)" }}
        >
          pull requests
        </h1>
        <Badge variant="outline">{pulls.length} total</Badge>
      </div>

      {pulls.length === 0 ? (
        <p className="mt-10 text-sm text-kumo-inactive">
          no pull requests yet.
        </p>
      ) : (
        <ul className="mt-10 divide-y divide-kumo-hairline border border-kumo-hairline">
          {pulls.map((pull) => (
            <li key={pull.number}>
              <Link
                href={`/repos/${name}/pulls/${pull.number}`}
                className="flex items-baseline justify-between gap-4 px-4 py-3 transition-colors hover:bg-kumo-elevated"
              >
                <span className="min-w-0">
                  <span className="flex items-baseline gap-3">
                    <span className="truncate text-sm">{pull.title}</span>
                    <Badge variant="outline">
                      {pull.merged ? "merged" : pull.state}
                    </Badge>
                    {!pull.merged &&
                      pull.state === "open" &&
                      !pull.mergeable && (
                        <Badge variant="outline">conflict</Badge>
                      )}
                  </span>
                  <span className="mt-0.5 block text-xs text-kumo-inactive">
                    #{pull.number} · {pull.user.login} · {pull.head.ref} →{" "}
                    {pull.base.ref} · head {shortId(pull.head.sha)}
                  </span>
                </span>
                <span className="shrink-0 text-xs text-kumo-inactive">
                  {timeAgo(Date.parse(pull.updated_at))}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
