import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";

import { api } from "src/lib/api";

export const dynamic = "force-dynamic";

export default async function Home() {
  const [orgs, repos] = await Promise.all([
    api().orgs().catch(() => null),
    api().listRepos().catch(() => null),
  ]);

  return (
    <div className="mx-auto max-w-6xl px-6 py-16">
      <div className="flex flex-wrap items-center justify-between gap-4 border-b border-kumo-hairline pb-6">
        <div className="flex flex-wrap items-center gap-4">
          <h1
            className="text-balance leading-[1.2]"
            style={{ fontSize: "clamp(1.5rem, 3vw, 1.875rem)" }}
          >
            dashboard
          </h1>
          <Badge variant="outline">
            {repos ? `${repos.length} repos` : "gateway unreachable"}
          </Badge>
        </div>
        <Link href="/repos/new">
          <Button type="button">new repository</Button>
        </Link>
      </div>

      <p className="mt-4 max-w-xl text-sm text-kumo-subtle">
        Clotho-owned orgs and repositories. Forgejo stays behind the gateway as
        the internal collaboration provider.
      </p>

      {repos === null ? (
        <p className="mt-10 text-sm text-kumo-subtle">
          could not reach the api gateway — is the dev stack up? (`just dev`)
        </p>
      ) : (
        <div className="mt-10 grid gap-8 lg:grid-cols-[240px_minmax(0,1fr)]">
          {orgs && orgs.length > 0 && (
            <section>
              <h2 className="text-sm">orgs</h2>
              <ul className="mt-3 space-y-2 text-sm">
                {orgs.map((org) => (
                  <li key={org.name} className="text-kumo-inactive">
                    {org.display_name || org.name}
                    <span className="ml-2 text-[11px] text-kumo-subtle">
                      {org.forgejo_owner}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          )}

          <section>
            <h2 className="text-sm">repositories</h2>
            {repos.length === 0 ? (
              <p className="mt-3 border border-kumo-hairline px-4 py-6 text-sm text-kumo-inactive">
                no repos yet. create one from the &quot;new repository&quot; button.
              </p>
            ) : (
              <ul className="mt-3 divide-y divide-kumo-hairline border border-kumo-hairline">
                {repos.map((repo) => (
                  <li key={repo.name}>
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
                        <span className="text-[11px] text-kumo-subtle">
                          {repo.visibility}
                        </span>
                      </span>
                      <span className="flex items-center gap-3 text-xs text-kumo-inactive">
                        {repo.owner && (
                          <span className="text-kumo-subtle">{repo.owner}</span>
                        )}
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
          </section>
        </div>
      )}
    </div>
  );
}
