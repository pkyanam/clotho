import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";
import type { ActivityEvent, ComputeProvider, RepoInfo } from "@clotho/sdk-js";

import { api, timeAgo } from "src/lib/api";
import {
  EmptyState,
  PageFrame,
  PageTitle,
  Panel,
  SectionHeader,
  StatCell,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

function isoToMillis(iso: string): number {
  const t = Date.parse(iso);
  return Number.isFinite(t) ? t : 0;
}

export default async function Home() {
  const [orgs, repos, activity, providerList] = await Promise.all([
    api().orgs().catch(() => null),
    api().listRepos().catch(() => null),
    api()
      .activity({ limit: 12 })
      .catch(() => [] as ActivityEvent[]),
    api()
      .computeProviderList()
      .catch(() => ({ providers: [] as ComputeProvider[], default_provider_id: "" })),
  ]);

  const gatewayDown = repos === null;
  const repoList = repos ?? [];
  const providers = providerList.providers;
  const configuredProviders = providers.filter((p) => p.configured);
  const unconfigured = providers.filter((p) => !p.configured);

  // Prefer the bootstrap / primary org; hide noisy stage-test names by default
  // when there are more than a handful — still searchable via orgs page later.
  const orgList = (orgs ?? []).filter((o) => {
    if ((orgs?.length ?? 0) <= 8) return true;
    const n = o.name.toLowerCase();
    return !n.includes("stage") && !/test-\d{10,}/.test(n);
  });

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
              value={orgs?.length ?? 0}
            />
            <StatCell
              label="activity"
              value={activity.length > 0 ? "live" : "quiet"}
              muted={activity.length === 0}
            />
          </div>

          <div className="mt-10 grid gap-8 lg:grid-cols-[minmax(0,1fr)_300px]">
            <div className="min-w-0 space-y-10">
              <section>
                <SectionHeader
                  title="repositories"
                  meta={`${repoList.length} total`}
                  actions={
                    <Link
                      href="/repos"
                      className="text-[0.8125rem] text-kumo-inactive hover:text-kumo-default"
                    >
                      view all
                    </Link>
                  }
                />
                {repoList.length === 0 ? (
                  <div className="mt-4">
                    <EmptyState
                      title="no repositories yet"
                      description="create a repository to start collaborating with humans and agents."
                      action={
                        <Link href="/repos/new">
                          <Button type="button">create repository</Button>
                        </Link>
                      }
                    />
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
                <SectionHeader title="compute health" meta="providers" />
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
                ) : (
                  <ul className="mt-4 grid gap-3 sm:grid-cols-2">
                    {providers.map((p) => (
                      <li
                        key={p.id}
                        className="border border-kumo-hairline bg-kumo-base px-4 py-3"
                      >
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="text-[0.9375rem]">{p.name}</span>
                          <Badge variant="outline">
                            {p.configured ? "configured" : "not connected"}
                          </Badge>
                          {p.enabled && <Badge variant="outline">default</Badge>}
                        </div>
                        <p className="mt-2 text-[0.8125rem] text-kumo-inactive">
                          {p.configured
                            ? p.configured_reason || "ready for Actions"
                            : p.configured_reason || "add credentials in settings"}
                        </p>
                      </li>
                    ))}
                  </ul>
                )}
                {unconfigured.length > 0 && configuredProviders.length === 0 && (
                  <p className="mt-3 text-[0.8125rem] text-kumo-inactive">
                    tip: open{" "}
                    <Link
                      href="/settings/compute"
                      className="underline hover:text-kumo-default"
                    >
                      compute settings
                    </Link>{" "}
                    to connect daytona without host env files.
                  </p>
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
                    {activity.map((ev) => (
                      <li key={ev.id} className="text-[0.8125rem]">
                        <span className="block text-kumo-default">
                          {formatEvent(ev)}
                        </span>
                        <span className="text-kumo-inactive">
                          {timeAgo(isoToMillis(ev.created_at))}
                        </span>
                      </li>
                    ))}
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

function formatEvent(ev: ActivityEvent): string {
  const payload = (ev.payload ?? {}) as Record<string, unknown>;
  switch (ev.event_type) {
    case "repo.created":
      return `repository ${String(payload.repo_name ?? "")} created`;
    case "org.created":
      return `organization ${String(payload.org_name ?? "")} created`;
    case "secret.created":
      return `secret ${String(payload.name ?? "")} created`;
    case "secret.updated":
      return `secret ${String(payload.name ?? "")} rotated`;
    case "secret.deleted":
      return `secret ${String(payload.name ?? "")} deleted`;
    case "provider.connected":
      return `provider ${String(payload.provider ?? "")} connected`;
    default:
      return ev.event_type.replace(/\./g, " ");
  }
}
