import Link from "next/link";
import { Badge, Button } from "@cloudflare/kumo";
import { ClothoApiError, type ActionsConfig } from "@clotho/sdk-js";

import { api, shortId, timeAgo } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import { runAction } from "./actions";

export const dynamic = "force-dynamic";

export default async function ActionsPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const client = api();
  const [detail, runs, config, providers] = await Promise.all([
    client.getRepo(name),
    client.actionRuns(name).catch(() => []),
    client.actionsConfig(name).catch((e) => fallbackActionsConfig(e)),
    client.computeProviders().catch(() => []),
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
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="actions" />
      <div className="mt-6 flex flex-wrap items-start justify-between gap-4 border-b border-kumo-hairline pb-5">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <h1 className="text-2xl leading-tight">actions</h1>
            <Badge variant="outline">{config.enabled ? "enabled" : "disabled"}</Badge>
            <Badge variant="outline">
              {provider?.configured ? "provider configured" : "provider missing"}
            </Badge>
          </div>
          <p className="mt-2 max-w-3xl text-sm text-kumo-subtle">
            Clotho runs `.clotho/ci.sh` when present. Without it, the runner
            probes Makefile, Cargo, then npm and syncs the final result back to
            pull-request commit statuses.
          </p>
          {!config.enabled && (
            <p className="mt-3 max-w-2xl border border-kumo-hairline px-3 py-2 text-xs text-kumo-inactive">
              actions config is not available from the api gateway currently
              serving this page.
            </p>
          )}
        </div>
        <form action={startRun}>
          <Button type="submit" disabled={!detail.main_commit_id || !config.enabled}>
            run
          </Button>
        </form>
      </div>

      <div className="mt-6 grid gap-4 md:grid-cols-4">
        <Summary label="provider" value={provider?.name ?? config.provider} />
        <Summary
          label="configured"
          value={provider?.configured ? "yes" : "no"}
          muted={!provider?.configured}
        />
        <Summary
          label="default image"
          value={config.default_image || "provider default"}
        />
        <Summary
          label="latest"
          value={latest ? `${latest.status} · ${timeAgo(latest.created_at_millis)}` : "none"}
          muted={!latest}
        />
      </div>

      <div className="mt-6 grid gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
        <section className="min-w-0">
          <div className="flex flex-wrap items-center justify-between gap-3 border border-kumo-hairline px-4 py-3">
            <div>
              <h2 className="text-sm">runs</h2>
              <p className="mt-1 text-xs text-kumo-inactive">
                {runs.length} retained · {active} active · {failures} needing attention
              </p>
            </div>
            <Badge variant="outline">postgres history</Badge>
          </div>
      {runs.length === 0 ? (
        <p className="border-x border-b border-kumo-hairline px-4 py-8 text-sm text-kumo-inactive">
          no action runs yet. start a manual run from the current main commit,
          or push a commit to let the webhook create one.
        </p>
      ) : (
        <ul className="divide-y divide-kumo-hairline border-x border-b border-kumo-hairline">
          {runs.map((run) => (
            <li key={run.id}>
              <Link
                href={`/repos/${name}/actions/${run.id}`}
                className="block px-4 py-3 transition-colors hover:bg-kumo-muted"
              >
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <StatusBadge status={run.status} />
                      <span className="text-sm">{run.id}</span>
                      <span className="text-xs text-kumo-inactive">
                        {run.trigger} by {run.actor}
                      </span>
                    </div>
                    <p className="mt-1 break-all text-xs text-kumo-inactive">
                      {shortId(run.commit_id)} · {run.provider}
                      {run.sandbox_id ? ` · sandbox ${run.sandbox_id}` : ""}
                    </p>
                  </div>
                  <span className="text-xs text-kumo-inactive">
                    {timeAgo(run.created_at_millis)}
                  </span>
                </div>
              </Link>
            </li>
          ))}
        </ul>
      )}
        </section>

        <aside className="space-y-4">
          <section className="border border-kumo-hairline p-4">
            <h2 className="text-sm">workflow resolution</h2>
            <ol className="mt-4 space-y-3 text-xs text-kumo-subtle">
              <li>1. detect `.clotho/ci.sh` and run it as the repo-owned contract</li>
              <li>2. otherwise probe Makefile, Cargo, then npm</li>
              <li>3. allocate a sandbox through the configured CCI provider</li>
              <li>4. ship git objects, check out the target commit, run checks</li>
              <li>5. persist logs and sync `clotho-ci` status to pull requests</li>
            </ol>
          </section>

          <section className="border border-kumo-hairline p-4">
            <h2 className="text-sm">runner policy</h2>
            <dl className="mt-4 grid gap-3 text-xs">
              <Meta label="timeout" value={`${config.timeout_seconds} seconds`} />
              <Meta label="trigger" value="push webhook or manual start" />
              <Meta label="status target" value="pull request commit status" />
              <Meta
                label="capabilities"
                value={(provider?.capabilities ?? []).join(", ") || "unknown"}
              />
            </dl>
          </section>
        </aside>
      </div>
    </div>
  );
}

function fallbackActionsConfig(error: unknown): ActionsConfig {
  if (error instanceof ClothoApiError && error.status === 404) {
    return {
      enabled: false,
      provider: "daytona",
      default_image: "ubuntu:22.04",
      timeout_seconds: 900,
    };
  }
  throw error;
}

function StatusBadge({ status }: { status: string }) {
  return <Badge variant="outline">{status}</Badge>;
}

function Summary({
  label,
  value,
  muted = false,
}: {
  label: string;
  value: string;
  muted?: boolean;
}) {
  return (
    <section className="border border-kumo-hairline p-4">
      <h2 className="text-xs text-kumo-inactive">{label}</h2>
      <p
        className={`mt-2 break-all text-sm ${muted ? "text-kumo-inactive" : "text-kumo-default"}`}
      >
        {value}
      </p>
    </section>
  );
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[112px_minmax(0,1fr)]">
      <dt className="text-kumo-inactive">{label}</dt>
      <dd className="min-w-0 break-words">{value}</dd>
    </div>
  );
}
