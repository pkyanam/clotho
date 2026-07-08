import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, formatBytes, shortId, timeAgo } from "src/lib/api";
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

  const conflictedCount = tree.files.filter((f) => f.conflicted).length;

  return (
    <div className="mx-auto max-w-6xl px-6 py-16">
      <RepoNav
        name={name}
        active="files"
        forgejoUrl={detail.forgejo.html_url}
      />

      <div className="mt-8 flex flex-wrap items-center gap-4">
        <h1
          className="leading-[1.2]"
          style={{ fontSize: "clamp(1.5rem, 3vw, 1.875rem)" }}
        >
          {name}
        </h1>
        {detail.main_commit_id && (
          <Badge variant="outline">
            main @ {shortId(detail.main_commit_id)}
          </Badge>
        )}
        {detail.heads.length > 1 && (
          <Badge variant="outline">{detail.heads.length} live heads</Badge>
        )}
        {conflictedCount > 0 && (
          <Badge variant="outline">{conflictedCount} conflicted</Badge>
        )}
      </div>
      {detail.forgejo.description && (
        <p className="mt-3 max-w-xl text-sm text-kumo-subtle">
          {detail.forgejo.description}
        </p>
      )}

      <div className="mt-10 flex flex-col gap-8 lg:flex-row">
        <div className="min-w-0 grow space-y-10">
          <section>
            <h2 className="text-xs text-kumo-subtle">
              files at {tree.commit_id ? shortId(tree.commit_id) : "main"}
            </h2>
            {tree.files.length === 0 ? (
              <p className="mt-3 text-sm text-kumo-inactive">
                empty tree — nothing woven yet.
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
            <h2 className="text-xs text-kumo-subtle">commits</h2>
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
          <PresencePanel repo={name} />

          <section className="border border-kumo-hairline p-4">
            <h2 className="text-xs text-kumo-subtle">operation log</h2>
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
