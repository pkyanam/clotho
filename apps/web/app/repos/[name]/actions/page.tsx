import Link from "next/link";
import { Badge, Button } from "@cloudflare/kumo";
import { ClothoApiError, type ActionsConfig } from "@clotho/sdk-js";

import { api, shortId, timeAgo } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import {
  EmptyState,
  PageFrame,
  Panel,
  SectionHeader,
  StatCell,
} from "src/components/ui/page-frame";
import { runAction } from "./actions";

export const dynamic = "force-dynamic";

export default async function ActionsPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const client = await api();
  const [detail, runs, config, providers, releases] = await Promise.all([
    client.getRepo(name),
    client.actionRuns(name).catch(() => []),
    client.actionsConfig(name).catch((e) => fallbackActionsConfig(e)),
    client.computeProviders().catch(() => []),
    client.releases(name).catch(() => []),
  ]);
  const provider = providers.find((p) => p.id === config.provider);
  const startRun = runAction.bind(null, name);
  const latest = runs[0];
  const failures = runs.filter((run) =>
    ["failure", "error"].includes(run.status),
  ).length;
  const active = runs.filter((run) =>
    ["queued", "running"].includes(run.status),
  ).length;

  return (
    <PageFrame>
      <RepoNav name={name} active="actions" />

      <div className="mt-6 flex flex-wrap items-start justify-between gap-4 border-b border-kumo-hairline pb-6">
        <div className="min-w-0 max-w-3xl">
          <div className="flex flex-wrap items-center gap-2">
            <h1
              className="leading-tight text-kumo-default"
              style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
            >
              actions
            </h1>
            <Badge variant="outline">
              {config.enabled ? "enabled" : "disabled"}
            </Badge>
          </div>
          <p className="mt-2 text-[0.9375rem] leading-relaxed text-kumo-inactive">
            clotho runs <code className="text-kumo-default">.clotho/ci.sh</code>{" "}
            when present. without it, the runner probes Makefile, Cargo, then
            npm and reports status on pull requests.
          </p>
        </div>
        <form action={startRun}>
          <Button
            type="submit"
            disabled={!detail.main_commit_id || !config.enabled || !provider?.configured}
          >
            run
          </Button>
        </form>
      </div>

      <div className="mt-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
        <StatCell label="provider" value={provider?.name ?? config.provider} />
        <StatCell
          label="status"
          value={provider?.configured ? "connected" : "not connected"}
          muted={!provider?.configured}
        />
        <StatCell
          label="accelerator"
          value={config.accelerator === "gpu" ? "GPU" : "CPU"}
        />
        <StatCell
          label="default image"
          value={
            config.accelerator === "gpu" && config.provider === "daytona"
              ? "daytona-gpu"
              : config.default_image || "provider default"
          }
        />
        <StatCell
          label="latest run"
          value={
            latest
              ? `${latest.status} · ${timeAgo(latest.created_at_millis)}`
              : "none"
          }
          muted={!latest}
        />
      </div>

      <div className="mt-8 grid gap-8 lg:grid-cols-[minmax(0,1fr)_320px]">
        <section className="min-w-0">
          <SectionHeader
            title="runs"
            meta={`${runs.length} retained · ${active} active · ${failures} needing attention`}
          />
          {runs.length === 0 ? (
            <div className="mt-4">
              <EmptyState
                title="no action runs yet"
                description="start a manual run from the current main commit, or push a change to trigger one automatically."
                action={
                  detail.main_commit_id && config.enabled && provider?.configured ? (
                    <form action={startRun}>
                      <Button type="submit">start first run</Button>
                    </form>
                  ) : (
                    <Link href="/settings/compute">
                      <Button type="button" variant="outline">
                        connect compute
                      </Button>
                    </Link>
                  )
                }
              />
            </div>
          ) : (
            <ul className="mt-4 divide-y divide-kumo-hairline border border-kumo-hairline">
              {runs.map((run) => (
                <li key={run.id}>
                  <Link
                    href={`/repos/${name}/actions/${run.id}`}
                    className="block px-4 py-3 transition-colors hover:bg-kumo-elevated"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-3">
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2">
                          <Badge variant="outline">{run.status}</Badge>
                          <span className="text-[0.9375rem] text-kumo-default">
                            {run.id}
                          </span>
                          <span className="text-[0.8125rem] text-kumo-inactive">
                            {run.trigger} by {run.actor}
                          </span>
                          {run.workflow !== "ci" && (
                            <Badge variant="outline">{run.workflow}</Badge>
                          )}
                          {run.release_version && (
                            <Badge variant="outline">{run.release_version}</Badge>
                          )}
                        </div>
                        <p className="mt-1 break-all text-[0.8125rem] text-kumo-inactive">
                          {shortId(run.commit_id)} · {run.provider}
                        </p>
                      </div>
                      <span className="text-[0.8125rem] text-kumo-inactive">
                        {timeAgo(run.created_at_millis)}
                      </span>
                    </div>
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </section>

        <aside className="space-y-5">
          <Panel className="p-4">
            <h2 className="text-[0.9375rem] font-medium text-kumo-default">
              release workloads
            </h2>
            <p className="mt-2 text-[0.8125rem] leading-relaxed text-kumo-inactive">
              run evaluation, inference, or benchmarks against an immutable
              release. Clotho injects the release version, commit, and verified
              manifest digest into the sandbox.
            </p>
            {releases.length === 0 ? (
              <p className="mt-4 border-t border-kumo-hairline pt-4 text-[0.8125rem] text-kumo-inactive">
                create a publishable release from the repository overview first.
              </p>
            ) : (
              <form action={startRun} className="mt-4 grid gap-3">
                <label className="text-[0.75rem] text-kumo-inactive">
                  workflow
                  <select
                    name="workflow"
                    className="mt-1 block w-full border border-kumo-hairline bg-kumo-control px-3 py-2 text-[0.8125rem] text-kumo-default"
                  >
                    <option value="evaluate">evaluate</option>
                    <option value="inference">inference</option>
                    <option value="benchmark">benchmark</option>
                  </select>
                </label>
                <label className="text-[0.75rem] text-kumo-inactive">
                  immutable release
                  <select
                    name="release_version"
                    required
                    className="mt-1 block w-full border border-kumo-hairline bg-kumo-control px-3 py-2 text-[0.8125rem] text-kumo-default"
                  >
                    {releases.map((release) => (
                      <option key={release.id} value={release.version}>
                        {release.version} · {shortId(release.commit_id)}
                      </option>
                    ))}
                  </select>
                </label>
                <Button
                  type="submit"
                  disabled={!config.enabled || !provider?.configured}
                >
                  run pinned workload
                </Button>
              </form>
            )}
          </Panel>

          <Panel className="p-4">
            <h2 className="text-[0.9375rem] font-medium text-kumo-default">
              workflow
            </h2>
            <ol className="mt-4 space-y-3 text-[0.8125rem] leading-relaxed text-kumo-inactive">
              <li>
                1. run <code>.clotho/ci.sh</code> when present
              </li>
              <li>2. otherwise probe Makefile, Cargo, then npm</li>
              <li>3. allocate a sandbox on the configured compute provider</li>
              <li>4. check out the target commit and run checks</li>
              <li>5. persist logs and update pull request status</li>
            </ol>
            {!provider?.configured && (
              <p className="mt-4 border-t border-kumo-hairline pt-4 text-[0.8125rem] text-kumo-inactive">
                provider not connected —{" "}
                <Link
                  href="/settings/compute"
                  className="underline hover:text-kumo-default"
                >
                  connect compute
                </Link>
              </p>
            )}
          </Panel>

          <Panel className="p-4">
            <h2 className="text-[0.9375rem] font-medium text-kumo-default">
              runner policy
            </h2>
            <dl className="mt-4 grid gap-3 text-[0.8125rem]">
              <Meta label="timeout" value={`${config.timeout_seconds} seconds`} />
              <Meta label="trigger" value="push or manual start" />
              <Meta label="status target" value="pull request checks" />
              <Meta
                label="GPU preference"
                value={config.gpu_types.join(", ") || "provider default"}
              />
              <Meta
                label="capabilities"
                value={(provider?.capabilities ?? []).join(", ") || "unknown"}
              />
            </dl>
          </Panel>
        </aside>
      </div>
    </PageFrame>
  );
}

function fallbackActionsConfig(error: unknown): ActionsConfig {
  if (error instanceof ClothoApiError && error.status === 404) {
    return {
      enabled: false,
      provider: "daytona",
      default_image: "ubuntu:22.04",
      timeout_seconds: 900,
      accelerator: "cpu",
      gpu_types: [],
    };
  }
  throw error;
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[112px_minmax(0,1fr)]">
      <dt className="text-kumo-inactive">{label}</dt>
      <dd className="min-w-0 break-words text-kumo-default">{value}</dd>
    </div>
  );
}
