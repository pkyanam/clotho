import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, cloneUrl, formatBytes, shortId, timeAgo } from "src/lib/api";
import { PresencePanel } from "src/components/presence-panel";
import { RepoNav } from "src/components/repo-nav";

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
  const [pulls, issues, branches, statuses, sessions] = await Promise.all([
    client.pulls(name, "open").catch(() => []),
    client.issues(name, "open").catch(() => []),
    client.branches(name).catch(() => []),
    detail.main_commit_id
      ? client.commitStatuses(name, detail.main_commit_id).catch(() => [])
      : Promise.resolve([]),
    client.agentSessions(name, { limit: 6, withinSecs: 86400 }).catch(() => []),
  ]);

  const conflictedCount = tree.files.filter((f) => f.conflicted).length;
  const latestCommit = commits[0];
  const failedChecks = statuses.filter((s) =>
    ["failure", "error"].includes(s.state),
  ).length;
  const passingChecks = statuses.filter((s) => s.state === "success").length;

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="code" />

      <div className="mt-6 grid gap-4 border-b border-kumo-hairline pb-6 lg:grid-cols-[minmax(0,1fr)_360px]">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-3">
            <h1 className="text-2xl leading-tight">
          {name}
        </h1>
            <Badge variant="outline">{detail.forgejo.default_branch}</Badge>
            {failedChecks > 0 ? (
              <Badge variant="outline">{failedChecks} failing checks</Badge>
            ) : passingChecks > 0 ? (
              <Badge variant="outline">{passingChecks} checks passing</Badge>
            ) : (
              <Badge variant="outline">checks pending</Badge>
            )}
            {conflictedCount > 0 && (
              <Badge variant="outline">{conflictedCount} conflicts</Badge>
            )}
          </div>
          {detail.forgejo.description && (
            <p className="mt-3 max-w-3xl text-sm text-kumo-subtle">
              {detail.forgejo.description}
            </p>
          )}
          <div className="mt-4 grid gap-2 text-xs text-kumo-inactive sm:grid-cols-2 lg:grid-cols-4">
            <span>clone {cloneUrl(detail.owner, name)}</span>
            <span>
              latest{" "}
              {latestCommit ? shortId(latestCommit.commit_id) : "unborn"}
            </span>
            <span>{pulls.length} open prs</span>
            <span>{issues.length} open issues</span>
          </div>
        </div>
        <div className="grid grid-cols-2 gap-2 text-xs">
          <SummaryCell label="branches" value={branches.length} />
          <SummaryCell label="active agents" value={sessions.length} />
          <SummaryCell label="files" value={tree.files.length} />
          <SummaryCell
            label="storage"
            value={formatBytes(
              tree.files.reduce((total, file) => total + file.size_bytes, 0),
            )}
          />
        </div>
      </div>

      <div className="mt-8 flex flex-col gap-8 lg:flex-row">
        <div className="min-w-0 grow space-y-10">
          <section>
            <SectionHeader
              title="code"
              meta={`files at ${tree.commit_id ? shortId(tree.commit_id) : "main"}`}
            />
            {tree.files.length === 0 ? (
              <p className="mt-3 border border-kumo-hairline px-4 py-6 text-sm text-kumo-inactive">
                empty tree. commit files through the cli, api, or an mcp agent.
              </p>
            ) : (
              <ul className="mt-3 divide-y divide-kumo-hairline border border-kumo-hairline">
                {tree.files.map((file) => (
                  <li key={file.path}>
                    <Link
                      href={`/repos/${name}/blob/${file.path}`}
                      className="flex items-baseline justify-between gap-4 px-4 py-2 text-sm transition-colors hover:bg-kumo-elevated"
                    >
                      <span className="flex items-baseline gap-3">
                        {file.path}
                        {file.conflicted && (
                          <Badge variant="outline">conflict</Badge>
                        )}
                      </span>
                      <span className="shrink-0 text-xs text-kumo-inactive">
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
            <ul className="mt-3 divide-y divide-kumo-hairline border border-kumo-hairline">
              {commits.map((commit) => (
                <li
                  key={commit.commit_id}
                  className="flex items-baseline justify-between gap-4 px-4 py-2"
                >
                  <span className="min-w-0">
                    <span className="block truncate text-sm">
                      {commit.description.split("\n")[0] || "(no description)"}
                    </span>
                    <span className="mt-0.5 block text-xs text-kumo-inactive">
                      {commit.author_name} · change {shortId(commit.change_id)}
                    </span>
                  </span>
                  <span className="shrink-0 text-xs text-kumo-inactive">
                    {shortId(commit.commit_id)} ·{" "}
                    {timeAgo(commit.timestamp_millis)}
                  </span>
                </li>
              ))}
            </ul>
          </section>
        </div>

        <div className="w-full shrink-0 space-y-6 lg:w-[340px]">
          <section className="border border-kumo-hairline p-4">
            <SectionHeader title="collaboration" meta="native facade" />
            <div className="mt-4 grid gap-2 text-xs">
              <DashboardLink href={`/repos/${name}/pulls`} label="pull requests" value={pulls.length} />
              <DashboardLink href={`/repos/${name}/issues`} label="issues" value={issues.length} />
              <DashboardLink href={`/repos/${name}/checks`} label="checks" value={statuses.length} />
              <DashboardLink href={`/repos/${name}/agents`} label="agents" value={sessions.length} />
            </div>
          </section>
          <PresencePanel repo={name} />

          <section className="border border-kumo-hairline p-4">
            <SectionHeader title="operation log" meta="jj timeline" />
            <ul className="mt-3 space-y-2">
              {opLog.map((op) => (
                <li key={op.operation_id} className="text-xs">
                  <span className="block truncate">{op.description}</span>
                  <span className="text-kumo-inactive">
                    {shortId(op.operation_id)} · {timeAgo(op.end_time_millis)}
                  </span>
                </li>
              ))}
            </ul>
          </section>
        </div>
      </div>
    </div>
  );
}

function SummaryCell({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="border border-kumo-hairline px-3 py-2">
      <div className="text-[11px] text-kumo-inactive">{label}</div>
      <div className="mt-1 truncate text-sm">{value}</div>
    </div>
  );
}

function SectionHeader({ title, meta }: { title: string; meta: string }) {
  return (
    <div className="flex items-baseline justify-between gap-3">
      <h2 className="text-sm">{title}</h2>
      <span className="text-xs text-kumo-inactive">{meta}</span>
    </div>
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
      className="flex items-center justify-between border border-kumo-hairline px-3 py-2 hover:bg-kumo-elevated"
    >
      <span>{label}</span>
      <Badge variant="outline">{value}</Badge>
    </Link>
  );
}
