import { Badge } from "@cloudflare/kumo";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, shortId, timeAgo } from "src/lib/api";
import { LogActions } from "src/components/log-actions";
import { RepoNav } from "src/components/repo-nav";

export const dynamic = "force-dynamic";

export default async function ActionRunPage({
  params,
}: {
  params: Promise<{ name: string; runId: string }>;
}) {
  const { name, runId } = await params;
  const client = api();
  const [run, log] = await Promise.all([
    client.actionRun(name, runId).catch((e) => {
      if (e instanceof ClothoApiError && e.status === 404) notFound();
      throw e;
    }),
    client.actionLogs(name, runId).catch(() => ({ run_id: runId, text: "" })),
  ]);
  const timeline = timelineFor(run);
  const logText = log.text || "logs have not been written yet.";

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="actions" />
      <div className="mt-6 flex flex-wrap items-center gap-3 border-b border-kumo-hairline pb-5">
        <h1 className="min-w-0 text-2xl leading-tight">{run.id}</h1>
        <StatusBadge status={run.status} />
        {run.conclusion && <Badge variant="outline">{run.conclusion}</Badge>}
      </div>
      <p className="mt-2 break-all text-xs text-kumo-inactive">
        {shortId(run.commit_id)} · {run.trigger} by {run.actor} · created{" "}
        {timeAgo(run.created_at_millis)}
      </p>

      <div className="mt-6 grid gap-4 md:grid-cols-5">
        <Summary label="provider" value={run.provider || "unknown"} />
        <Summary label="sandbox" value={run.sandbox_id || "not assigned"} />
        <Summary label="branch" value={run.branch || "main"} />
        <Summary label="trigger" value={`${run.trigger} by ${run.actor}`} />
        <Summary
          label="duration"
          value={run.duration_ms ? formatDuration(run.duration_ms) : "pending"}
        />
      </div>

      <div className="mt-8 grid gap-6 lg:grid-cols-[320px_minmax(0,1fr)]">
        <aside className="space-y-6">
          <section className="border border-kumo-hairline p-4">
            <h2 className="text-sm">timeline</h2>
            <ol className="mt-4 space-y-3">
              {timeline.map((item) => (
                <li key={item.label} className="flex gap-3 text-xs">
                  <span
                    className={`mt-1 h-2 w-2 shrink-0 rounded-full border border-kumo-hairline ${
                      item.done ? "bg-kumo-default" : "bg-kumo-base"
                    }`}
                  />
                  <div>
                    <p className={item.done ? "text-kumo-default" : "text-kumo-inactive"}>
                      {item.label}
                    </p>
                    <p className="mt-1 text-kumo-inactive">{item.detail}</p>
                  </div>
                </li>
              ))}
            </ol>
          </section>

          <section className="border border-kumo-hairline">
            <div className="border-b border-kumo-hairline px-4 py-3">
              <h2 className="text-sm">jobs</h2>
            </div>
            <ul className="divide-y divide-kumo-hairline">
              {run.jobs.map((job) => (
                <li key={job.id} className="px-4 py-3 text-sm">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <span>{job.name}</span>
                    <Badge variant="outline">{job.status}</Badge>
                  </div>
                  <dl className="mt-3 grid gap-2 text-xs text-kumo-inactive">
                    <Meta label="exit" value={job.exit_code !== null ? String(job.exit_code) : "pending"} />
                    <Meta label="provider" value={run.provider || "unknown"} />
                    <Meta label="sandbox" value={run.sandbox_id || "not assigned"} />
                    <Meta label="commit" value={shortId(run.commit_id)} />
                  </dl>
                </li>
              ))}
            </ul>
          </section>
        </aside>

        <section className="min-w-0 border border-kumo-hairline">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-kumo-hairline px-4 py-3">
            <div>
              <h2 className="text-sm">logs</h2>
              <p className="mt-1 text-xs text-kumo-inactive">
                fixed-width output from checkout, script selection, command execution,
                and status sync.
              </p>
            </div>
            <LogActions text={logText} filename={`${name}-${run.id}.log`} />
          </div>
          <pre
            className={`max-h-[680px] overflow-auto whitespace-pre-wrap break-words p-4 font-mono text-xs leading-relaxed ${
              run.status === "failure" || run.status === "error"
                ? "text-kumo-default"
                : "text-kumo-subtle"
            }`}
          >
            {logText}
          </pre>
        </section>
      </div>
    </div>
  );
}

function Summary({ label, value }: { label: string; value: string }) {
  return (
    <section className="border border-kumo-hairline p-4">
      <h2 className="text-xs text-kumo-inactive">{label}</h2>
      <p className="mt-2 break-all text-sm">{value}</p>
    </section>
  );
}

function StatusBadge({ status }: { status: string }) {
  return <Badge variant="outline">{status}</Badge>;
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid grid-cols-[72px_minmax(0,1fr)] gap-2">
      <dt>{label}</dt>
      <dd className="min-w-0 break-all text-kumo-subtle">{value}</dd>
    </div>
  );
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)} s`;
  return `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`;
}

function timelineFor(run: {
  status: string;
  sandbox_id: string;
  started_at_millis: number;
  finished_at_millis: number;
}) {
  const running = run.started_at_millis > 0;
  const finished = run.finished_at_millis > 0;
  return [
    {
      label: "queued",
      detail: "run record created and commit status marked pending",
      done: true,
    },
    {
      label: "sandbox allocated",
      detail: run.sandbox_id || "waiting for provider assignment",
      done: Boolean(run.sandbox_id),
    },
    {
      label: "repo unpacked",
      detail: "git object archive shipped through the compute interface",
      done: running,
    },
    {
      label: "checkout",
      detail: "target commit selected inside the sandbox checkout",
      done: running,
    },
    {
      label: "script",
      detail: ".clotho/ci.sh or fallback probe executed",
      done: finished,
    },
    {
      label: "status sync",
      detail: "final result persisted and sent to the PR status surface",
      done: finished,
    },
  ];
}
