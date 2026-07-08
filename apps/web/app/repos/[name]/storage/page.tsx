import { Badge } from "@cloudflare/kumo";

import { api, formatBytes } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";

export const dynamic = "force-dynamic";

export default async function StoragePage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const tree = await api().tree(name);
  const totalBytes = tree.files.reduce((total, file) => total + file.size_bytes, 0);
  const largest = [...tree.files]
    .sort((a, b) => b.size_bytes - a.size_bytes)
    .slice(0, 20);

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="storage" />
      <div className="mt-6 flex flex-wrap items-center gap-3">
        <h1 className="text-2xl leading-tight">storage</h1>
        <Badge variant="outline">{formatBytes(totalBytes)}</Badge>
        <Badge variant="outline">{tree.files.length} files</Badge>
      </div>
      <p className="mt-2 text-sm text-kumo-subtle">
        repository file footprint at the current main tree.
      </p>
      <ul className="mt-8 divide-y divide-kumo-hairline border border-kumo-hairline">
        {largest.map((file) => (
          <li
            key={file.path}
            className="flex items-baseline justify-between gap-4 px-4 py-2 text-sm"
          >
            <span className="truncate">{file.path}</span>
            <span className="shrink-0 text-xs text-kumo-inactive">
              {formatBytes(file.size_bytes)}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
