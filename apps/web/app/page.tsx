import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";
import type { ActivityEvent, ComputeProvider, RepoInfo } from "@clotho/sdk-js";

import { loadAgentSummary } from "src/lib/agents-summary";
import { api, timeAgo } from "src/lib/api";
import { formatEvent, isoToMillis } from "src/lib/events";
import { filterSeedOrgs, filterSeedRepos } from "src/lib/seed-noise";
import {
  EmptyState,
  PageFrame,
  PageTitle,
  Panel,
  SectionHeader,
  StatCell,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function Home({
  searchParams,
}: {
  searchParams: Promise<{ show?: string }>;
}) {
  const { show } = await searchParams;
  const showAll = show === "all";

  const client = await api();
  const [orgs, repos, activity, providerList] = await Promise.all([
    client.orgs().catch(() => null),
    client.listRepos().catch(() => null),
    client
      .activity({ limit: 12 })
      .catch(() => [] as ActivityEvent[]),
    client
      .computeProviderList()
      .catch(() => ({ providers: [] as ComputeProvider[], default_provider_id: "" })),
  ]);

  const gatewayDown = repos === null;
  const rawRepoList = repos ?? [];
  const repoList = filterSeedRepos(rawRepoList, showAll);
  const hiddenRepoCount = rawRepoList.length - repoList.length;
  const orgList = filterSeedOrgs(orgs ?? [], showAll);

  const providers = providerList.providers;
  const configuredProviders = providers.filter((p) => p.configured);
  const unconfiguredProviders = providers.filter((p) => !p.configured);
  const daytona = providers.find((p) => p.id === "daytona");

  const agentSummary = gatewayDown
    ? null
    : await loadAgentSummary(repoList.length > 0 ? repoList : rawRepoList);

  return (
    <PageFrame>
      <PageTitle
        title="dashboard"
        description="repositories, compute, agents, and recent activity — your control plane for humans and agents."
        actions={
          <>
            <Link href="/repos/new">
              <Button type="button">new repository</Button>
            </Link>
            {configuredProviders.length === 0 && (
              <Link href="/settings/compute">
                <Button type="button" variant="outline">
                  connect compute
                </Button>
              </Link>
            )}
          </>
        }
      />

      {gatewayDown ? (
        <div className="mt-10">
          <EmptyState
            title="api gateway unreachable"
            description="start the local stack with just dev, then refresh. the web app talks to the gateway on port 8080."
            action={
              <Link href="/">
                <Button type="button" variant="outline">
                  retry
                </Button>
              </Link>
            }
          />
        </div>
      ) : (
        <>
          <div className="mt-8 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <StatCell label="repositories" value={repoList.length} />
            <StatCell
              label="compute"
              value={
                configuredProviders.length > 0
                  ? `${configuredProviders.length} connected`
                  : "not connected"
              }
              muted={configuredProviders.length === 0}
            />
            <StatCell
              label="organizations"
              value={orgList.length}
            />
            <StatCell
              label="agents · 7d"
              value={
                agentSummary && agentSummary.identities > 0
                  ? agentSummary.identities
                  : "none"
              }
              muted={!agentSummary || agentSummary.identities === 0}
            />
          </div>

          <div className="mt-10 grid gap-8 lg:grid-cols-[minmax(0,1fr)_300px]">
            <div className="min-w-0 space-y-10">
              <section>
                <SectionHeader
                  title="repositories"
                  meta={
                    hiddenRepoCount > 0 && !showAll
                      ? `${repoList.length} shown`
                      : `${repoList.length} total`
                  }
                  actions={
                    <div className="flex flex-wrap items-center gap-3">
                      {hiddenRepoCount > 0 && !showAll && (
                        <Link
                          href="/?show=all"
                          className="text-[0.8125rem] text-kumo-inactive hover:text-kumo-default"
                        >
                          show all including test
                        </Link>
                      )}
                      {showAll && hiddenRepoCount > 0 && (
                        <Link
                          href="/"
                          className="text-[0.8125rem] text-kumo-inactive hover:text-kumo-default"
                        >
                          hide test repos
                        </Link>
                      )}
                      <Link
                        href="/repos"
                        className="text-[0.8125rem] text-kumo-inactive hover:text-kumo-default"
                      >
                        view all
                      </Link>
                    </div>
                  }
                />
                {repoList.length === 0 ? (
                  <div className="mt-4">
                    {rawRepoList.length > 0 && !showAll ? (
                      <EmptyState
                        title="no product repositories yet"
                        description="seed and stage test repos are hidden by default. create a repository or show all to see test data."
                        action={
                          <div className="flex flex-wrap justify-center gap-2">
                            <Link href="/repos/new">
                              <Button type="button">create repository</Button>
                            </Link>
                            <Link href="/?show=all">
                              <Button type="button" variant="outline">
                                show all including test
                              </Button>
                            </Link>
                          </div>
                        }
                      />
                    ) : (
                      <EmptyState
                        title="no repositories yet"
                        description="create a repository to start collaborating with humans and agents."
                        action={
                          <Link href="/repos/new">
                            <Button type="button">create repository</Button>
                          </Link>
                        }
                      />
                    )}
                  </div>
                ) : (
                  <ul className="mt-4 divide-y divide-kumo-hairline border border-kumo-hairline">
                    {repoList.slice(0, 12).map((repo) => (
                      <RepoRow key={repo.name} repo={repo} />
                    ))}
                  </ul>
                )}
              </section>

              <section>
                <SectionHeader
                  title="compute health"
                  meta={
                    configuredProviders.length > 0
                      ? `${configuredProviders.length} connected`
                      : "not connected"
                  }
                  actions={
                    <Link
                      href="/settings/compute"
                      className="text-[0.8125rem] text-kumo-inactive hover:text-kumo-default"
                    >
                      settings
                    </Link>
                  }
                />
                {providers.length === 0 ? (
                  <div className="mt-4">
                    <EmptyState
                      title="no providers reported"
                      description="connect a compute provider to run Actions and agent sandboxes."
                      action={
                        <Link href="/settings/compute">
                          <Button type="button">open compute settings</Button>
                        </Link>
                      }
                    />
                  </div>
                ) : configuredProviders.length === 0 ? (
                  <div className="mt-4 border border-kumo-hairline bg-kumo-base px-5 py-6">
                    <p className="text-[0.9375rem] text-kumo-default">
                      connect compute to run Actions and agent sandboxes
                    </p>
                    <p className="mt-2 max-w-lg text-[0.8125rem] leading-relaxed text-kumo-inactive">
                      daytona is the recommended starting provider — paste an api
                      key once in settings and clotho stores it as an org secret.
                    </p>
                    <div className="mt-5 flex flex-wrap items-center gap-3">
                      <Link href="/settings/compute">
                        <Button type="button">connect daytona</Button>
                      </Link>
                      {unconfiguredProviders.length > 1 && (
                        <Link
                          href="/settings/compute"
                          className="text-[0.8125rem] text-kumo-inactive underline hover:text-kumo-default"
                        >
                          other providers ({unconfiguredProviders.length - 1})
                        </Link>
                      )}
                    </div>
                  </div>
                ) : (
                  <>
                    <ul className="mt-4 divide-y divide-kumo-hairline border border-kumo-hairline">
                      {configuredProviders.map((p) => (
                        <ComputeRow key={p.id} provider={p} prominent />
                      ))}
                    </ul>
                    {unconfiguredProviders.length > 0 && (
                      <p className="mt-3 text-[0.8125rem] text-kumo-inactive">
                        {unconfiguredProviders.length} more provider
                        {unconfiguredProviders.length === 1 ? "" : "s"} available —{" "}
                        <Link
                          href="/settings/compute"
                          className="underline hover:text-kumo-default"
                        >
                          connect in settings
                        </Link>
                        {daytona && !daytona.configured && (
                          <> (including {daytona.name})</>
                        )}
                      </p>
                    )}
                  </>
                )}
              </section>
            </div>

            <aside className="space-y-6">
              {orgList.length > 0 && (
                <Panel className="p-4">
                  <SectionHeader title="organizations" meta={`${orgList.length}`} />
                  <ul className="mt-4 space-y-2">
                    {orgList.map((org) => (
                      <li key={org.name}>
                        <Link
                          href={`/orgs/${org.name}`}
                          className="block text-[0.875rem] text-kumo-default hover:underline"
                        >
                          {org.display_name || org.name}
                        </Link>
                      </li>
                    ))}
                  </ul>
                </Panel>
              )}

              <Panel className="p-4">
                <SectionHeader
                  title="agents"
                  meta="7d"
                  actions={
                    <Link
                      href="/agents"
                      className="text-[0.8125rem] text-kumo-inactive hover:text-kumo-default"
                    >
                      manage agents
                    </Link>
                  }
                />
                {!agentSummary || agentSummary.sessionCount === 0 ? (
                  <p className="mt-4 text-[0.8125rem] leading-relaxed text-kumo-inactive">
                    no agent sessions yet. agents connect with scoped tokens and
                    show up here when they start working.{" "}
                    <Link href="/agents" className="underline hover:text-kumo-default">
                      view agents
                    </Link>
                  </p>
                ) : (
                  <dl className="mt-4 space-y-2 text-[0.8125rem]">
                    <div className="flex justify-between gap-2">
                      <dt className="text-kumo-inactive">identities</dt>
                      <dd className="text-kumo-default">{agentSummary.identities}</dd>
                    </div>
                    <div className="flex justify-between gap-2">
                      <dt className="text-kumo-inactive">recent sessions</dt>
                      <dd className="text-kumo-default">{agentSummary.sessionCount}</dd>
                    </div>
                    {agentSummary.lastSeenMs !== null && (
                      <div className="flex justify-between gap-2">
                        <dt className="text-kumo-inactive">last activity</dt>
                        <dd className="text-kumo-default">
                          {timeAgo(agentSummary.lastSeenMs)}
                        </dd>
                      </div>
                    )}
                  </dl>
                )}
              </Panel>

              <Panel className="p-4">
                <SectionHeader
                  title="activity"
                  meta="recent"
                  actions={
                    <Link
                      href="/activity"
                      className="text-[0.8125rem] text-kumo-inactive hover:text-kumo-default"
                    >
                      all
                    </Link>
                  }
                />
                {activity.length === 0 ? (
                  <p className="mt-4 text-[0.8125rem] text-kumo-inactive">
                    no events yet. create a repo or connect a provider to get started.
                  </p>
                ) : (
                  <ul className="mt-4 space-y-3">
                    {activity.map((ev) => {
                      const formatted = formatEvent(ev);
                      return (
                        <li key={ev.id} className="text-[0.8125rem]">
                          {formatted.href ? (
                            <Link
                              href={formatted.href}
                              className="block text-kumo-default hover:underline"
                            >
                              {formatted.text}
                            </Link>
                          ) : (
                            <span className="block text-kumo-default">
                              {formatted.text}
                            </span>
                          )}
                          <span className="text-kumo-inactive">
                            {timeAgo(isoToMillis(ev.created_at))}
                          </span>
                        </li>
                      );
                    })}
                  </ul>
                )}
              </Panel>

              <Panel className="p-4">
                <SectionHeader title="quick actions" />
                <ul className="mt-4 space-y-2 text-[0.875rem]">
                  <li>
                    <Link href="/repos/new" className="hover:underline">
                      new repository
                    </Link>
                  </li>
                  <li>
                    <Link href="/settings/compute" className="hover:underline">
                      connect compute
                    </Link>
                  </li>
                  <li>
                    <Link href="/settings/secrets" className="hover:underline">
                      manage secrets
                    </Link>
                  </li>
                  <li>
                    <Link href="/agents" className="hover:underline">
                      view agents
                    </Link>
                  </li>
                </ul>
              </Panel>
            </aside>
          </div>
        </>
      )}
    </PageFrame>
  );
}

function RepoRow({ repo }: { repo: RepoInfo }) {
  return (
    <li>
      <Link
        href={`/repos/${repo.name}`}
        className="flex flex-wrap items-center justify-between gap-3 px-4 py-3.5 transition-colors hover:bg-kumo-elevated"
      >
        <span className="min-w-0">
          <span className="flex flex-wrap items-center gap-2">
            <span className="text-[0.9375rem] text-kumo-default">{repo.name}</span>
            <Badge variant="outline">{repo.visibility}</Badge>
            {repo.configured && (
              <Badge variant="outline">{repo.provider}</Badge>
            )}
          </span>
          {repo.description && (
            <span className="mt-1 block truncate text-[0.8125rem] text-kumo-inactive">
              {repo.description}
            </span>
          )}
        </span>
        <span className="flex shrink-0 flex-wrap items-center gap-3 text-[0.8125rem] text-kumo-inactive">
          {repo.owner && <span>{repo.owner}</span>}
          {repo.open_pr_counter > 0 && <span>{repo.open_pr_counter} prs</span>}
          {repo.open_issues_count > 0 && (
            <span>{repo.open_issues_count} issues</span>
          )}
          <span>{repo.default_branch}</span>
        </span>
      </Link>
    </li>
  );
}

function ComputeRow({
  provider,
  prominent = false,
}: {
  provider: ComputeProvider;
  prominent?: boolean;
}) {
  return (
    <li className={prominent ? "bg-kumo-base px-4 py-3" : "px-4 py-3 opacity-80"}>
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-[0.9375rem] text-kumo-default">{provider.name}</span>
        <Badge variant="outline">
          {provider.configured ? "configured" : "not connected"}
        </Badge>
        {provider.enabled && <Badge variant="outline">default</Badge>}
      </div>
      <p className="mt-1.5 text-[0.8125rem] text-kumo-inactive">
        {provider.configured
          ? provider.configured_reason || "ready for Actions"
          : provider.configured_reason || "add credentials in settings"}
      </p>
    </li>
  );
}
