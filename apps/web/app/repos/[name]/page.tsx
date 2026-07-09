import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import {
  api,
  formatBytes,
  publicCloneUrl,
  shortId,
  timeAgo,
} from "src/lib/api";
import { PresencePanel } from "src/components/presence-panel";
import { RepoNav } from "src/components/repo-nav";
import {
  EmptyState,
  PageFrame,
  Panel,
  SectionHeader,
  StatCell,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function RepoPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const client = api();

  const detail = await client.getRepo(name).catch((e) => {
    if (e instanceof ClothoApiError && e.status === 404) notFound();
    throw e;
  });
  const [tree, commits, opLog] = await Promise.all([
    client.tree(name),
    client.commits(name, { limit: 20 }),
    client.opLog(name, 20),
  ]);
  const [pulls, issues, branches, statuses, sessions, actionRuns] =
    await Promise.all([
      client.pulls(name, "open").catch(() => []),
      client.issues(name, "open").catch(() => []),
      client.branches(name).catch(() => []),
      detail.main_commit_id
        ? client.commitStatuses(name, detail.main_commit_id).catch(() => [])
        : Promise.resolve([]),
      client.agentSessions(name, { limit: 6, withinSecs: 86400 }).catch(() => []),
      client.actionRuns(name, { limit: 5 }).catch(() => []),
    ]);

  const conflictedCount = tree.files.filter((f) => f.conflicted).length;
  const latestCommit = commits[0];
  const failedActions = statuses.filter((s) =>
    ["failure", "error"].includes(s.state),
  ).length;
  const passingActions = statuses.filter((s) => s.state === "success").length;
  const description =
    detail.forgejo?.description ||
    (detail as { description?: string }).description ||
    "";
  const clone = publicCloneUrl(detail.clone_url, detail.owner, name);
  const storageBytes = tree.files.reduce((t, f) => t + f.size_bytes, 0);

  let actionsLabel = "actions idle";
  if (failedActions > 0) actionsLabel = `${failedActions} failing`;
  else if (passingActions > 0) actionsLabel = `${passingActions} passing`;
  else if (actionRuns.some((r) => ["queued", "running"].includes(r.status))) {
    actionsLabel = "actions running";
  }

  return (
    <PageFrame>
      <RepoNav name={name} active="code" />

      <div className="mt-6 grid gap-6 border-b border-kumo-hairline pb-6 lg:grid-cols-[minmax(0,1fr)_280px]">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h1
              className="leading-tight text-kumo-default"
              style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
            >
              {name}
            </h1>
          </div>
          {description && (
            <p className="mt-3 max-w-3xl text-[0.9375rem] leading-relaxed text-kumo-inactive">
              {description}
            </p>
          )}

          {/* Status strip — designed chips, not badge soup */}
          <div className="mt-5 flex flex-wrap gap-2">
            <StatusChip label="branch" value={detail.default_branch} />
            <StatusChip label="visibility" value={detail.visibility} />
            <StatusChip
              label="compute"
              value={detail.configured ? detail.provider : "not connected"}
              tone={detail.configured ? "ok" : "warn"}
            />
            <StatusChip
              label="actions"
              value={actionsLabel}
              tone={
                failedActions > 0 ? "bad" : passingActions > 0 ? "ok" : "neutral"
              }
            />
            {sessions.length > 0 && (
              <StatusChip
                label="agents"
                value={`${sessions.length} active`}
                tone="ok"
              />
            )}
            {conflictedCount > 0 && (
              <StatusChip
                label="conflicts"
                value={String(conflictedCount)}
                tone="bad"
              />
            )}
          </div>

          <div className="mt-5 flex flex-wrap items-center gap-3 text-[0.8125rem] text-kumo-inactive">
            <code className="border border-kumo-hairline bg-kumo-base px-2 py-1 text-kumo-default">
              {clone}
            </code>
            {latestCommit && (
              <span>
                latest {shortId(latestCommit.commit_id)} ·{" "}
                {timeAgo(latestCommit.timestamp_millis)}
              </span>
            )}
          </div>
        </div>

        <div className="grid grid-cols-2 gap-2">
          <StatCell label="branches" value={branches.length} />
          <StatCell label="agents" value={sessions.length} />
          <StatCell label="files" value={tree.files.length} />
          <StatCell label="storage" value={formatBytes(storageBytes)} />
        </div>
      </div>

      <div className="mt-8 flex flex-col gap-8 lg:flex-row">
        <div className="min-w-0 grow space-y-10">
          <section>
            <SectionHeader
              title="code"
              meta={
                tree.commit_id
                  ? `at ${shortId(tree.commit_id)}`
                  : detail.default_branch
              }
            />
            {tree.files.length === 0 ? (
              <div className="mt-4">
                <EmptyState
                  title="empty repository"
                  description="push your first commit with the clotho cli or sdk, or let an agent open a session and write the first change."
                />
              </div>
            ) : (
              <ul className="mt-4 divide-y divide-kumo-hairline border border-kumo-hairline">
                {tree.files.map((file) => (
                  <li key={file.path}>
                    <Link
                      href={`/repos/${name}/blob/${file.path}`}
                      className="flex items-baseline justify-between gap-4 px-4 py-2.5 text-[0.875rem] transition-colors hover:bg-kumo-elevated"
                    >
                      <span className="flex items-baseline gap-3">
                        {file.path}
                        {file.conflicted && (
                          <Badge variant="outline">conflict</Badge>
                        )}
                      </span>
                      <span className="shrink-0 text-[0.8125rem] text-kumo-inactive">
                        {formatBytes(file.size_bytes)}
                      </span>
                    </Link>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section>
            <SectionHeader title="commits" meta={`${commits.length} recent`} />
            {commits.length === 0 ? (
              <p className="mt-4 border border-kumo-hairline px-4 py-6 text-[0.875rem] text-kumo-inactive">
                no commits yet.
              </p>
            ) : (
              <ul className="mt-4 divide-y divide-kumo-hairline border border-kumo-hairline">
                {commits.map((commit) => (
                  <li
                    key={commit.commit_id}
                    className="flex items-baseline justify-between gap-4 px-4 py-3"
                  >
                    <span className="min-w-0">
                      <span className="block truncate text-[0.875rem]">
                        {commit.description.split("\n")[0] || "(no description)"}
                      </span>
                      <span className="mt-0.5 block text-[0.8125rem] text-kumo-inactive">
                        {commit.author_name} · {shortId(commit.change_id)}
                      </span>
                    </span>
                    <span className="shrink-0 text-[0.8125rem] text-kumo-inactive">
                      {shortId(commit.commit_id)} ·{" "}
                      {timeAgo(commit.timestamp_millis)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>

        <div className="w-full shrink-0 space-y-5 lg:w-[320px]">
          <Panel className="p-4">
            <SectionHeader title="collaboration" />
            <div className="mt-4 grid gap-2">
              <DashboardLink
                href={`/repos/${name}/pulls`}
                label="pull requests"
                value={pulls.length}
              />
              <DashboardLink
                href={`/repos/${name}/issues`}
                label="issues"
                value={issues.length}
              />
              <DashboardLink
                href={`/repos/${name}/actions`}
                label="actions"
                value={actionRuns.length || statuses.length}
              />
              <DashboardLink
                href={`/repos/${name}/agents`}
                label="agents"
                value={sessions.length}
              />
            </div>
          </Panel>

          <PresencePanel repo={name} />

          <Panel className="p-4">
            <SectionHeader title="activity" meta="operation timeline" />
            {opLog.length === 0 ? (
              <p className="mt-3 text-[0.8125rem] text-kumo-inactive">
                no operations yet.
              </p>
            ) : (
              <ul className="mt-3 space-y-3">
                {opLog.map((op) => (
                  <li key={op.operation_id} className="text-[0.8125rem]">
                    <span className="block truncate text-kumo-default">
                      {op.description}
                    </span>
                    <span className="text-kumo-inactive">
                      {shortId(op.operation_id)} ·{" "}
                      {timeAgo(op.end_time_millis)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </Panel>
        </div>
      </div>
    </PageFrame>
  );
}

function StatusChip({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: string;
  tone?: "neutral" | "ok" | "warn" | "bad";
}) {
  const ring =
    tone === "ok"
      ? "border-white/40"
      : tone === "warn" || tone === "bad"
        ? "border-white/25"
        : "border-kumo-hairline";
  return (
    <span
      className={`inline-flex items-center gap-1.5 border px-2.5 py-1 text-[0.75rem] ${ring}`}
    >
      <span className="text-kumo-inactive">{label}</span>
      <span className="text-kumo-default">{value}</span>
    </span>
  );
}

function DashboardLink({
  href,
  label,
  value,
}: {
  href: string;
  label: string;
  value: number;
}) {
  return (
    <Link
      href={href}
      className="flex items-center justify-between border border-kumo-hairline px-3 py-2.5 text-[0.875rem] transition-colors hover:bg-kumo-elevated"
    >
      <span>{label}</span>
      <Badge variant="outline">{value}</Badge>
    </Link>
  );
}
