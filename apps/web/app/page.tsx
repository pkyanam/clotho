import { Badge } from "@cloudflare/kumo";
import Link from "next/link";

import { api } from "src/lib/api";

export const dynamic = "force-dynamic";

export default async function Home() {
  const repos = await api()
    .listRepos()
    .catch(() => null);

  return (
    <div className="mx-auto max-w-6xl px-6 py-16">
      <div className="flex flex-wrap items-center gap-4">
        <h1
          className="text-balance leading-[1.2]"
          style={{ fontSize: "clamp(1.5rem, 3vw, 1.875rem)" }}
        >
          repos
        </h1>
        <Badge variant="outline">
          {repos ? `${repos.length} woven` : "gateway unreachable"}
        </Badge>
      </div>

      <p className="mt-4 max-w-xl text-sm text-kumo-subtle">
        jj-native repositories, spun by humans and agents at once. every commit
        is a real git object; the operation log remembers everything.
      </p>

      <div className="rule-hairline mt-10" />

      {repos === null ? (
        <p className="mt-10 text-sm text-kumo-subtle">
          could not reach the api gateway — is the dev stack up? (`just dev`)
        </p>
      ) : repos.length === 0 ? (
        <p className="mt-10 text-sm text-kumo-subtle">
          no repos yet. create one: `curl -X POST localhost:8080/api/v1/repos -d{" "}
          {"'"}
          {"{"}&quot;name&quot;:&quot;weave&quot;{"}"}
          {"'"}`
        </p>
      ) : (
        <ul className="mt-10 divide-y divide-kumo-hairline border border-kumo-hairline">
          {repos.map((repo) => (
            <li key={repo.id}>
              <Link
                href={`/repos/${repo.name}`}
                className="flex items-baseline justify-between gap-4 px-4 py-3 transition-colors hover:bg-kumo-elevated"
              >
                <span className="flex items-baseline gap-3">
                  <span className="text-sm">{repo.name}</span>
                  {repo.description && (
                    <span className="text-xs text-kumo-inactive">
                      {repo.description}
                    </span>
                  )}
                </span>
                <span className="flex items-center gap-3 text-xs text-kumo-inactive">
                  {repo.open_pr_counter > 0 && (
                    <span>{repo.open_pr_counter} open prs</span>
                  )}
                  {repo.open_issues_count > 0 && (
                    <span>{repo.open_issues_count} issues</span>
                  )}
                  <span>{repo.default_branch}</span>
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
