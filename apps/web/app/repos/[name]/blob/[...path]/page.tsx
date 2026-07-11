import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, formatBytes, shortId } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import { PageFrame } from "src/components/ui/page-frame";
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

  const file = await (await api())
    .file(name, filePath, commit_id)
    .catch((e) => {
      if (e instanceof ClothoApiError && (e.status === 404 || e.status === 400))
        notFound();
      throw e;
    });

  const lines = file.content?.split("\n") ?? [];

  return (
    <PageFrame>
      <RepoNav name={name} active="code" />

      <div className="mt-6 border-b border-kumo-hairline pb-6">
        <div className="text-[0.8125rem] text-kumo-inactive">
          <Link href={`/repos/${name}`} className="hover:text-kumo-default">
            code
          </Link>{" "}
          / {filePath}
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-3">
          <h1 className="min-w-0 break-all text-[1.125rem] leading-tight text-kumo-default">
            {file.path}
          </h1>
          <Badge variant="outline">{formatBytes(file.size_bytes)}</Badge>
          <Badge variant="outline">at {shortId(file.commit_id)}</Badge>
          {file.executable && <Badge variant="outline">executable</Badge>}
          {file.conflicted && <Badge variant="outline">conflict</Badge>}
        </div>
      </div>

      {file.conflicted && (
        <p className="mt-6 max-w-3xl border border-kumo-hairline bg-kumo-base px-4 py-3 text-[0.875rem] leading-relaxed text-kumo-inactive">
          this file carries an unresolved conflict — a first-class object, not
          a blocker. the text below shows every side of the conflict; land a
          follow-up commit to resolve it.
        </p>
      )}

      {file.binary ? (
        <p className="mt-8 border border-kumo-hairline bg-kumo-base px-4 py-8 text-center text-[0.875rem] text-kumo-inactive">
          binary file — {formatBytes(file.size_bytes)}. no text preview.
        </p>
      ) : (
        <pre className="mt-6 overflow-x-auto border border-kumo-hairline bg-kumo-base text-[0.8125rem] leading-relaxed">
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
    </PageFrame>
  );
}
