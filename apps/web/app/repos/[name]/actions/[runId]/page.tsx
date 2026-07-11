import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, shortId, timeAgo } from "src/lib/api";
import { LogActions } from "src/components/log-actions";
import { RepoNav } from "src/components/repo-nav";
import { PageFrame, Panel, StatCell } from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function ActionRunPage({
  params,
}: {
  params: Promise<{ name: string; runId: string }>;
}) {
  const { name, runId } = await params;
  const client = await api();
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
    <PageFrame>
      <RepoNav name={name} active="actions" />

      <div className="mt-6 border-b border-kumo-hairline pb-6">
        <div className="text-[0.8125rem] text-kumo-inactive">
          <Link
            href={`/repos/${name}/actions`}
            className="hover:text-kumo-default"
          >
            actions
          </Link>{" "}
          / {run.id}
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-3">
          <h1
            className="min-w-0 break-all leading-tight text-kumo-default"
            style={{ fontSize: "clamp(1.25rem, 2.5vw, 1.5rem)" }}
          >
            {run.id}
          </h1>
          <Badge variant="outline">{run.status}</Badge>
          {run.conclusion && <Badge variant="outline">{run.conclusion}</Badge>}
        </div>
        <p className="mt-2 break-all text-[0.875rem] text-kumo-inactive">
          {shortId(run.commit_id)} · {run.trigger} by {run.actor} · created{" "}
          {timeAgo(run.created_at_millis)}
        </p>
      </div>

      <div className="mt-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
        <StatCell label="provider" value={run.provider || "unknown"} />
        <StatCell
          label="sandbox"
          value={run.sandbox_id || "not assigned"}
          muted={!run.sandbox_id}
        />
        <StatCell label="branch" value={run.branch || "main"} />
        <StatCell label="trigger" value={`${run.trigger} by ${run.actor}`} />
        <StatCell
          label="duration"
          value={run.duration_ms ? formatDuration(run.duration_ms) : "pending"}
          muted={!run.duration_ms}
        />
      </div>

      <div className="mt-8 grid gap-8 lg:grid-cols-[320px_minmax(0,1fr)]">
        <aside className="space-y-5">
          <Panel className="p-4">
            <h2 className="text-[0.9375rem] font-medium text-kumo-default">
              timeline
            </h2>
            <ol className="mt-4 space-y-3">
              {timeline.map((item) => (
                <li key={item.label} className="flex gap-3 text-[0.8125rem]">
                  <span
                    className={`mt-1 h-2 w-2 shrink-0 rounded-full border border-kumo-hairline ${
                      item.done ? "bg-kumo-default" : "bg-kumo-base"
                    }`}
                  />
                  <div>
                    <p
                      className={
                        item.done ? "text-kumo-default" : "text-kumo-inactive"
                      }
                    >
                      {item.label}
                    </p>
                    <p className="mt-1 text-kumo-inactive">{item.detail}</p>
                  </div>
                </li>
              ))}
            </ol>
          </Panel>

          <Panel>
            <div className="border-b border-kumo-hairline px-4 py-3">
              <h2 className="text-[0.9375rem] font-medium text-kumo-default">
                jobs
              </h2>
            </div>
            <ul className="divide-y divide-kumo-hairline">
              {run.jobs.map((job) => (
                <li key={job.id} className="px-4 py-3 text-[0.875rem]">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <span className="text-kumo-default">{job.name}</span>
                    <Badge variant="outline">{job.status}</Badge>
                  </div>
                  <dl className="mt-3 grid gap-2 text-[0.8125rem] text-kumo-inactive">
                    <Meta
                      label="exit"
                      value={
                        job.exit_code !== null ? String(job.exit_code) : "pending"
                      }
                    />
                    <Meta label="provider" value={run.provider || "unknown"} />
                    <Meta
                      label="sandbox"
                      value={run.sandbox_id || "not assigned"}
                    />
                    <Meta label="commit" value={shortId(run.commit_id)} />
                  </dl>
                </li>
              ))}
            </ul>
          </Panel>
        </aside>

        <section className="min-w-0 border border-kumo-hairline bg-kumo-base">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-kumo-hairline px-4 py-3">
            <div>
              <h2 className="text-[0.9375rem] font-medium text-kumo-default">
                logs
              </h2>
              <p className="mt-1 text-[0.8125rem] text-kumo-inactive">
                output from checkout, script selection, command execution, and
                status sync.
              </p>
            </div>
            <LogActions text={logText} filename={`${name}-${run.id}.log`} />
          </div>
          <pre
            className={`max-h-[680px] overflow-auto whitespace-pre-wrap break-words p-4 font-mono text-[0.8125rem] leading-relaxed ${
              run.status === "failure" || run.status === "error"
                ? "text-kumo-default"
                : "text-kumo-inactive"
            }`}
          >
            {logText}
          </pre>
        </section>
      </div>
    </PageFrame>
  );
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid grid-cols-[72px_minmax(0,1fr)] gap-2">
      <dt>{label}</dt>
      <dd className="min-w-0 break-all">{value}</dd>
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
      detail: "run recorded and commit status marked pending",
      done: true,
    },
    {
      label: "sandbox allocated",
      detail: run.sandbox_id || "waiting for provider assignment",
      done: Boolean(run.sandbox_id),
    },
    {
      label: "repo shipped",
      detail: "repository contents delivered to the sandbox",
      done: running,
    },
    {
      label: "checkout",
      detail: "target commit selected inside the sandbox",
      done: running,
    },
    {
      label: "script",
      detail: ".clotho/ci.sh or fallback probe executed",
      done: finished,
    },
    {
      label: "status sync",
      detail: "final result persisted and reported to pull requests",
      done: finished,
    },
  ];
}
