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
import { MarkdownCard } from "src/components/markdown-card";
import { RepoNav } from "src/components/repo-nav";
import { createRelease } from "./release-actions";
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
  const client = await api();

  const detail = await client.getRepo(name).catch((e) => {
    if (e instanceof ClothoApiError && e.status === 404) notFound();
    throw e;
  });
  const [tree, commits, opLog, storage, manifest, releases] = await Promise.all([
    client.tree(name),
    client.commits(name, { limit: 20 }),
    client.opLog(name, 20),
    client.storageStats(name),
    client.artifacts(name),
    client.releases(name),
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
    detail.description ||
    (detail as { description?: string }).description ||
    "";
  const clone = publicCloneUrl(detail.clone_url, detail.owner, name);
  const logicalFileBytes = new Map(
    storage.large_files.map((file) => [file.path, file.logical_bytes]),
  );
  const cardArtifact = manifest.artifacts.find(
    (artifact) => artifact.role === "card" && artifact.size_bytes <= 1024 * 1024,
  );
  const previewArtifact = manifest.artifacts.find(
    (artifact) =>
      artifact.role === "dataset_shard" &&
      ["csv", "tsv", "jsonl"].includes(artifact.format),
  );
  const [cardFile, datasetPreview] = await Promise.all([
    cardArtifact
      ? client.file(name, cardArtifact.path).catch(() => null)
      : Promise.resolve(null),
    detail.kind === "dataset" && previewArtifact
      ? client
          .artifactPreview(name, previewArtifact.path, { limit: 25 })
          .catch(() => null)
      : Promise.resolve(null),
  ]);
  const metadataChips = artifactMetadataChips(manifest.metadata);
  const latestRelease = releases[0]
    ? await client.release(name, releases[0].version).catch(() => null)
    : null;
  const evaluations = releaseEvaluations(latestRelease?.manifest.metadata ?? {});

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
            <StatusChip label="kind" value={detail.kind} />
            <StatusChip label="branch" value={detail.default_branch} />
            <StatusChip label="visibility" value={detail.visibility} />
            <StatusChip
              label="network"
              value={detail.network_mode}
              tone={detail.network_mode === "tailscale" ? "ok" : "neutral"}
            />
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
          <Link href={`/repos/${name}/branches`} className="block">
            <StatCell label="branches" value={branches.length} />
          </Link>
          <Link href={`/repos/${name}/agents`} className="block">
            <StatCell label="agents" value={sessions.length} />
          </Link>
          <StatCell label="files" value={tree.files.length} />
          <Link href={`/repos/${name}/storage`} className="block">
            <StatCell label="storage" value={formatBytes(storage.logical_bytes)} />
          </Link>
        </div>
      </div>

      <div className="mt-8 flex flex-col gap-8 lg:flex-row">
        <div className="min-w-0 grow space-y-10">
          {detail.kind !== "code" && (
            <section>
              <SectionHeader
                title={`${detail.kind} artifacts`}
                meta={manifest.readiness.ready ? "publishable" : "needs attention"}
              />
              <div className="mt-4 grid grid-cols-2 gap-2 sm:grid-cols-4">
                <StatCell label="payload" value={formatBytes(manifest.total_bytes)} />
                <StatCell label="artifacts" value={manifest.total_files} />
                <StatCell label="Arachne" value={manifest.arachne_files} />
                <StatCell
                  label="formats"
                  value={Object.keys(manifest.format_counts).length}
                />
              </div>
              <div className="mt-3 border border-kumo-hairline">
                <div className="flex flex-wrap items-center gap-2 border-b border-kumo-hairline px-4 py-3 text-[0.8125rem]">
                  <ReadinessItem label={`${detail.kind} card`} ready={manifest.readiness.card} />
                  <ReadinessItem
                    label={detail.kind === "model" ? "weights" : "dataset shards"}
                    ready={manifest.readiness.primary_artifacts}
                  />
                  <ReadinessItem label="metadata" ready={manifest.readiness.metadata} />
                  {Object.entries(manifest.format_counts).map(([format, count]) => (
                    <Badge key={format} variant="outline">
                      {format} · {count}
                    </Badge>
                  ))}
                </div>
                {metadataChips.length > 0 && (
                  <div className="flex flex-wrap items-center gap-2 border-b border-kumo-hairline px-4 py-3 text-[0.75rem]">
                    <span className="mr-1 text-kumo-inactive">discovery metadata</span>
                    {metadataChips.map(({ key, value }, index) => (
                      <Badge key={`${key}-${value}-${index}`} variant="outline">
                        {key} · {value}
                      </Badge>
                    ))}
                    <span className="text-kumo-inactive">
                      from {manifest.metadata_sources.join(", ")}
                    </span>
                  </div>
                )}
                {manifest.readiness.warnings.length > 0 && (
                  <ul className="border-b border-kumo-hairline bg-kumo-base px-4 py-2 text-[0.8125rem] text-kumo-inactive">
                    {manifest.readiness.warnings.map((warning) => (
                      <li key={warning}>→ {warning}</li>
                    ))}
                  </ul>
                )}
                <ul className="divide-y divide-kumo-hairline">
                  {manifest.artifacts.slice(0, 8).map((artifact) => (
                    <li key={artifact.path}>
                      <Link
                        href={`/repos/${name}/blob/${artifact.path}`}
                        className="flex items-center justify-between gap-4 px-4 py-2.5 text-[0.875rem] transition-colors hover:bg-kumo-elevated"
                      >
                        <span className="min-w-0 truncate">{artifact.path}</span>
                        <span className="flex shrink-0 items-center gap-2 text-[0.75rem] text-kumo-inactive">
                          <span>{artifact.role}</span>
                          <Badge variant="outline">{artifact.format}</Badge>
                          {artifact.storage === "arachne" && (
                            <Badge variant="outline">Arachne</Badge>
                          )}
                          <span>{formatBytes(artifact.size_bytes)}</span>
                        </span>
                      </Link>
                    </li>
                  ))}
                </ul>
              </div>
            </section>
          )}

          <section id="releases">
            <SectionHeader
              title="releases"
              meta={`${releases.length} immutable`}
            />
            <div className="mt-4 border border-kumo-hairline">
              {releases.length > 0 && (
                <ul className="divide-y divide-kumo-hairline">
                  {releases.map((release) => (
                    <li
                      key={release.id}
                      className="flex flex-wrap items-center justify-between gap-3 px-4 py-3 text-[0.8125rem]"
                    >
                      <span>
                        <strong className="font-medium text-kumo-default">
                          {release.version}
                        </strong>
                        <span className="ml-3 text-kumo-inactive">
                          {shortId(release.commit_id)} · {release.total_files} files ·{" "}
                          {formatBytes(release.total_bytes)}
                        </span>
                      </span>
                      <span className="flex items-center gap-2 text-kumo-inactive">
                        <Badge variant="outline">
                          {release.verified ? "✓ verified" : "invalid"}
                        </Badge>
                        <code>sha256:{shortId(release.manifest_sha256)}</code>
                      </span>
                    </li>
                  ))}
                </ul>
              )}
              {releases.length === 0 && (
                <p className="px-4 py-4 text-[0.8125rem] text-kumo-inactive">
                  no immutable releases yet. a release freezes this commit and its
                  semantic artifact manifest in Clotho.
                </p>
              )}
              <form
                action={createRelease.bind(null, name)}
                className="flex flex-wrap items-end gap-3 border-t border-kumo-hairline bg-kumo-base px-4 py-3"
              >
                <label className="min-w-48 grow text-[0.75rem] text-kumo-inactive">
                  version
                  <input
                    name="version"
                    required
                    maxLength={100}
                    pattern="[A-Za-z0-9][A-Za-z0-9._+\-]*"
                    placeholder="v1.0.0"
                    className="mt-1 block w-full border border-kumo-hairline bg-kumo-control px-3 py-2 text-[0.875rem] text-kumo-default outline-none focus:border-kumo-contrast"
                  />
                </label>
                <button
                  type="submit"
                  disabled={!manifest.readiness.ready}
                  className="border border-kumo-contrast px-4 py-2 text-[0.8125rem] text-kumo-default disabled:cursor-not-allowed disabled:opacity-40"
                >
                  create release
                </button>
                {!manifest.readiness.ready && (
                  <span className="w-full text-[0.75rem] text-kumo-inactive">
                    resolve artifact readiness warnings before publishing.
                  </span>
                )}
              </form>
              {latestRelease && (
                <div className="border-t border-kumo-hairline px-4 py-3">
                  <p className="text-[0.75rem] text-kumo-inactive">
                    frozen files · {latestRelease.version} · sha256:
                    {shortId(latestRelease.manifest_sha256)}
                  </p>
                  <ul className="mt-2 grid gap-1 sm:grid-cols-2">
                    {latestRelease.manifest.artifacts.slice(0, 10).map((artifact) => {
                      const encodedPath = artifact.path
                        .split("/")
                        .map(encodeURIComponent)
                        .join("/");
                      return (
                        <li key={artifact.path}>
                          <a
                            href={`/api/repos/${encodeURIComponent(name)}/releases/${encodeURIComponent(latestRelease.version)}/resolve/${encodedPath}`}
                            download
                            className="flex items-center justify-between gap-3 border border-kumo-hairline px-3 py-2 text-[0.8125rem] hover:bg-kumo-elevated"
                          >
                            <span className="min-w-0 truncate">{artifact.path}</span>
                            <span className="shrink-0 text-kumo-inactive">
                              {formatBytes(artifact.size_bytes)} ↓
                            </span>
                          </a>
                        </li>
                      );
                    })}
                  </ul>
                </div>
              )}
            </div>
          </section>

          {evaluations.length > 0 && latestRelease && (
            <section>
              <SectionHeader
                title="evaluations"
                meta={`${evaluations.length} frozen in ${latestRelease.version}`}
              />
              <ul className="mt-4 divide-y divide-kumo-hairline border border-kumo-hairline">
                {evaluations.map((evaluation) => {
                  const metrics =
                    evaluation.data.metrics && typeof evaluation.data.metrics === "object"
                      ? (evaluation.data.metrics as Record<string, unknown>)
                      : {};
                  return (
                    <li key={evaluation.path} className="px-4 py-3">
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <span className="text-[0.875rem]">{evaluation.path}</span>
                        <span className="flex flex-wrap gap-1.5">
                          {["task", "dataset", "hardware"].map((key) =>
                            evaluation.data[key] == null ? null : (
                              <Badge key={key} variant="outline">
                                {key} · {previewCell(evaluation.data[key])}
                              </Badge>
                            ),
                          )}
                          {Object.entries(metrics).slice(0, 8).map(([metric, value]) => (
                            <Badge key={metric} variant="outline">
                              {metric.replaceAll("_", " ")} · {previewCell(value)}
                            </Badge>
                          ))}
                        </span>
                      </div>
                      <p className="mt-2 font-mono text-[0.6875rem] text-kumo-inactive">
                        commit {shortId(latestRelease.commit_id)} · manifest {shortId(latestRelease.manifest_sha256)}
                      </p>
                    </li>
                  );
                })}
              </ul>
            </section>
          )}

          {cardFile?.content && (
            <section>
              <SectionHeader title={`${detail.kind} card`} meta={cardFile.path} />
              <div className="mt-4">
                <MarkdownCard repo={name} content={cardFile.content} />
              </div>
            </section>
          )}

          {datasetPreview && (
            <section>
              <SectionHeader
                title="dataset preview"
                meta={`${datasetPreview.rows.length} rows from ${datasetPreview.path}${datasetPreview.truncated ? " · bounded preview" : ""}`}
              />
              <div className="mt-4 overflow-x-auto border border-kumo-hairline">
                <table className="w-full border-collapse text-left text-[0.8125rem]">
                  <thead>
                    <tr className="bg-kumo-base">
                      {datasetPreview.columns.map((column) => (
                        <th
                          key={column}
                          className="whitespace-nowrap border-b border-r border-kumo-hairline px-3 py-2 font-medium last:border-r-0"
                        >
                          {column}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {datasetPreview.rows.map((row, rowIndex) => (
                      <tr key={rowIndex} className="border-b border-kumo-hairline last:border-b-0">
                        {datasetPreview.columns.map((column, columnIndex) => (
                          <td
                            key={`${column}-${columnIndex}`}
                            className="max-w-[24rem] truncate border-r border-kumo-hairline px-3 py-2 last:border-r-0"
                            title={previewCell(row[columnIndex])}
                          >
                            {previewCell(row[columnIndex])}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <p className="mt-2 text-[0.75rem] text-kumo-inactive">
                read {formatBytes(datasetPreview.bytes_read)} through Clotho; previews are capped at
                256 KiB and 100 rows.
              </p>
            </section>
          )}

          <section>
            <SectionHeader
              title="repository files"
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
                        {logicalFileBytes.has(file.path) && (
                          <Badge variant="outline">Arachne</Badge>
                        )}
                        {formatBytes(logicalFileBytes.get(file.path) ?? file.size_bytes)}
                      </span>
                    </Link>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section>
            <SectionHeader
              title="commits"
              meta={`${commits.length} recent`}
              actions={
                <Link
                  href={`/repos/${name}/commits`}
                  className="text-[0.8125rem] text-kumo-inactive hover:text-kumo-default"
                >
                  view all
                </Link>
              }
            />
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

function previewCell(value: unknown) {
  if (value === null || value === undefined) return "—";
  if (typeof value === "string") return value;
  if (typeof value === "number") {
    return value.toLocaleString("en-US", {
      maximumSignificantDigits: 6,
      useGrouping: false,
    });
  }
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

function releaseEvaluations(metadata: Record<string, unknown>) {
  if (!Array.isArray(metadata.evaluations)) return [];
  return metadata.evaluations.flatMap((entry) => {
    if (!entry || typeof entry !== "object") return [];
    const record = entry as Record<string, unknown>;
    if (typeof record.path !== "string" || !record.data || typeof record.data !== "object") {
      return [];
    }
    return [{ path: record.path, data: record.data as Record<string, unknown> }];
  });
}

function artifactMetadataChips(metadata: Record<string, unknown>) {
  const chips: Array<{ key: string; value: string }> = [];
  const pushValue = (key: string, value: unknown) => {
    if (chips.length >= 16 || value === null || value === undefined) return;
    if (["string", "number", "boolean"].includes(typeof value)) {
      chips.push({ key: key.replaceAll("_", " "), value: String(value) });
    } else if (Array.isArray(value)) {
      const summary = value
        .filter((item) => ["string", "number", "boolean"].includes(typeof item))
        .slice(0, 4)
        .join(", ");
      if (summary) chips.push({ key: key.replaceAll("_", " "), value: summary });
    }
  };
  const priority = [
    "license",
    "pipeline_tag",
    "library_name",
    "language",
    "datasets",
    "base_model",
    "task_categories",
    "size_categories",
    "tags",
    "metrics",
    "pretty_name",
  ];
  for (const key of priority) pushValue(key, metadata[key]);
  for (const [key, value] of Object.entries(metadata)) {
    if (priority.includes(key)) continue;
    if (value && typeof value === "object" && !Array.isArray(value)) {
      for (const [nestedKey, nestedValue] of Object.entries(value)) {
        pushValue(nestedKey, nestedValue);
      }
    } else {
      pushValue(key, value);
    }
  }
  return chips;
}

function ReadinessItem({ label, ready }: { label: string; ready: boolean }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span aria-hidden>{ready ? "✓" : "○"}</span>
      <span className={ready ? "text-kumo-default" : "text-kumo-inactive"}>
        {label}
      </span>
    </span>
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
      ? "border-kumo-contrast/40"
      : tone === "warn" || tone === "bad"
        ? "border-kumo-contrast/25"
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
