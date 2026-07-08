import { Badge } from "@cloudflare/kumo";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, formatBytes, shortId } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import { conflictLineKind } from "src/lib/conflicts";

export const dynamic = "force-dynamic";

export default async function BlobPage({
  params,
  searchParams,
}: {
  params: Promise<{ name: string; path: string[] }>;
  searchParams: Promise<{ commit_id?: string }>;
}) {
  const { name, path } = await params;
  const { commit_id } = await searchParams;
  const filePath = path.map(decodeURIComponent).join("/");

  const file = await api()
    .file(name, filePath, commit_id)
    .catch((e) => {
      if (e instanceof ClothoApiError && (e.status === 404 || e.status === 400))
        notFound();
      throw e;
    });

  const lines = file.content?.split("\n") ?? [];

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="code" />

      <div className="mt-8 flex flex-wrap items-center gap-3">
        <h1 className="text-lg">{file.path}</h1>
        <Badge variant="outline">{formatBytes(file.size_bytes)}</Badge>
        <Badge variant="outline">at {shortId(file.commit_id)}</Badge>
        {file.executable && <Badge variant="outline">executable</Badge>}
        {file.conflicted && <Badge variant="outline">conflict</Badge>}
      </div>

      {file.conflicted && (
        <p className="mt-4 max-w-2xl border border-kumo-hairline px-4 py-3 text-xs text-kumo-subtle">
          this file is an unresolved jj conflict — a first-class object, not a
          blocker. the text below is jj&apos;s materialization of every side;
          land a follow-up commit to resolve it.
        </p>
      )}

      {file.binary ? (
        <p className="mt-8 text-sm text-kumo-inactive">
          binary file — {formatBytes(file.size_bytes)}.
        </p>
      ) : (
        <pre className="mt-8 overflow-x-auto border border-kumo-hairline text-xs leading-relaxed">
          {lines.map((line, i) => {
            const marker = file.conflicted && conflictLineKind(line);
            return (
              <div
                key={i}
                className={`flex gap-4 px-2 ${marker ? "bg-kumo-elevated" : ""}`}
              >
                <span className="w-10 shrink-0 select-none text-right text-kumo-inactive">
                  {i + 1}
                </span>
                <code className={marker ? "font-bold" : ""}>{line}</code>
              </div>
            );
          })}
        </pre>
      )}
    </div>
  );
}
