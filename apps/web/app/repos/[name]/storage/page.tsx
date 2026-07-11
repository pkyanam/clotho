import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, formatBytes, shortId } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import {
  EmptyState,
  PageFrame,
  Panel,
  SectionHeader,
  StatCell,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function StoragePage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const tree = await (await api())
    .tree(name)
    .catch((e) => {
      if (e instanceof ClothoApiError && e.status === 404) notFound();
      throw e;
    });

  const totalBytes = tree.files.reduce((total, file) => total + file.size_bytes, 0);
  const largest = [...tree.files]
    .sort((a, b) => b.size_bytes - a.size_bytes)
    .slice(0, 25);
  const maxBytes = largest[0]?.size_bytes ?? 1;
  const conflicted = tree.files.filter((f) => f.conflicted).length;
  const byType = typeBreakdown(tree.files);

  return (
    <PageFrame>
      <RepoNav name={name} active="storage" />

      <div className="mt-6 border-b border-kumo-hairline pb-6">
        <h1
          className="leading-tight text-kumo-default"
          style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
        >
          storage
        </h1>
        <p className="mt-2 max-w-3xl text-[0.9375rem] leading-relaxed text-kumo-inactive">
          file footprint at the current main tree
          {tree.commit_id ? ` (${shortId(tree.commit_id)})` : ""}. deep object
          store metrics are not part of this view.
        </p>
      </div>

      {tree.files.length === 0 ? (
        <div className="mt-8">
          <EmptyState
            title="nothing stored yet"
            description="the footprint appears once the first commit lands on this repository."
          />
        </div>
      ) : (
        <>
          <div className="mt-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <StatCell label="total size" value={formatBytes(totalBytes)} />
            <StatCell label="files" value={tree.files.length} />
            <StatCell
              label="largest file"
              value={largest[0] ? formatBytes(largest[0].size_bytes) : "—"}
            />
            <StatCell
              label="conflicted"
              value={conflicted}
              muted={conflicted === 0}
            />
          </div>

          <div className="mt-8 grid gap-8 lg:grid-cols-[minmax(0,1fr)_280px]">
            <section className="min-w-0">
              <SectionHeader
                title="largest files"
                meta={`top ${largest.length} of ${tree.files.length}`}
              />
              <ul className="mt-4 divide-y divide-kumo-hairline border border-kumo-hairline">
                {largest.map((file) => (
                  <li key={file.path}>
                    <Link
                      href={`/repos/${name}/blob/${file.path}`}
                      className="block px-4 py-2.5 transition-colors hover:bg-kumo-elevated"
                    >
                      <span className="flex items-baseline justify-between gap-4">
                        <span className="flex min-w-0 items-baseline gap-2">
                          <span className="truncate text-[0.875rem] text-kumo-default">
                            {file.path}
                          </span>
                          {file.conflicted && (
                            <Badge variant="outline">conflict</Badge>
                          )}
                        </span>
                        <span className="shrink-0 text-[0.8125rem] text-kumo-inactive">
                          {formatBytes(file.size_bytes)}
                        </span>
                      </span>
                      <span className="mt-1.5 block h-px w-full bg-kumo-elevated">
                        <span
                          className="block h-px bg-kumo-contrast/40"
                          style={{
                            width: `${Math.max(
                              (file.size_bytes / maxBytes) * 100,
                              1,
                            )}%`,
                          }}
                        />
                      </span>
                    </Link>
                  </li>
                ))}
              </ul>
            </section>

            <aside>
              <Panel className="p-4">
                <h2 className="text-[0.9375rem] font-medium text-kumo-default">
                  by file type
                </h2>
                <ul className="mt-4 space-y-2.5">
                  {byType.map((row) => (
                    <li
                      key={row.ext}
                      className="flex items-baseline justify-between gap-3 text-[0.8125rem]"
                    >
                      <span className="text-kumo-default">{row.ext}</span>
                      <span className="text-kumo-inactive">
                        {row.count} {row.count === 1 ? "file" : "files"} ·{" "}
                        {formatBytes(row.bytes)}
                      </span>
                    </li>
                  ))}
                </ul>
              </Panel>
            </aside>
          </div>
        </>
      )}
    </PageFrame>
  );
}

function typeBreakdown(
  files: Array<{ path: string; size_bytes: number }>,
): Array<{ ext: string; count: number; bytes: number }> {
  const map = new Map<string, { count: number; bytes: number }>();
  for (const file of files) {
    const base = file.path.split("/").pop() ?? file.path;
    const dot = base.lastIndexOf(".");
    const ext = dot > 0 ? base.slice(dot) : "(no extension)";
    const entry = map.get(ext) ?? { count: 0, bytes: 0 };
    entry.count += 1;
    entry.bytes += file.size_bytes;
    map.set(ext, entry);
  }
  return [...map.entries()]
    .map(([ext, v]) => ({ ext, ...v }))
    .sort((a, b) => b.bytes - a.bytes)
    .slice(0, 10);
}
