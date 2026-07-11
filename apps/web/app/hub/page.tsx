import { Badge, Button } from "@cloudflare/kumo";
import type { HubCatalogEntry } from "@clotho/sdk-js";
import Link from "next/link";

import { EmptyState, PageFrame, PageTitle } from "src/components/ui/page-frame";
import { api, formatBytes, timeAgo } from "src/lib/api";

export const dynamic = "force-dynamic";

type HubPageProps = {
  searchParams: Promise<{ q?: string; kind?: string }>;
};

export default async function HubPage({ searchParams }: HubPageProps) {
  const params = await searchParams;
  const query = params.q?.trim() ?? "";
  const kind = params.kind === "datasets" ? "datasets" : "models";
  const client = await api();
  const entries = await (kind === "models"
    ? client.hubModels({ search: query || undefined })
    : client.hubDatasets({ search: query || undefined })
  ).catch(() => null as HubCatalogEntry[] | null);

  return (
    <PageFrame>
      <PageTitle
        title="hub"
        description="models and datasets whose immutable releases are owned, verified, and distributed by Clotho."
        actions={
          <Link href="/repos/new">
            <Button type="button">new repository</Button>
          </Link>
        }
      />

      <div className="mt-7 flex flex-wrap items-center gap-2 border-b border-kumo-hairline pb-4">
        {(["models", "datasets"] as const).map((tab) => (
          <Link
            key={tab}
            href={`/hub?kind=${tab}${query ? `&q=${encodeURIComponent(query)}` : ""}`}
            className={`border px-3 py-2 text-[0.8125rem] transition-colors ${
              kind === tab
                ? "border-kumo-contrast text-kumo-default"
                : "border-kumo-hairline text-kumo-inactive hover:text-kumo-default"
            }`}
          >
            {tab}
          </Link>
        ))}
        <form className="ml-auto flex min-w-64 flex-1 gap-2 sm:max-w-lg" action="/hub">
          <input type="hidden" name="kind" value={kind} />
          <input
            name="q"
            defaultValue={query}
            placeholder={`search ${kind}, cards, tags, and libraries`}
            className="min-w-0 flex-1 border border-kumo-hairline bg-transparent px-3 py-2 text-[0.8125rem] outline-none placeholder:text-kumo-placeholder focus:border-kumo-contrast"
          />
          <Button type="submit" variant="secondary">
            search
          </Button>
        </form>
      </div>

      {entries === null ? (
        <div className="mt-10">
          <EmptyState
            title="could not load the hub"
            description="the Clotho catalog API may be unavailable. repository data remains untouched."
          />
        </div>
      ) : entries.length === 0 ? (
        <div className="mt-10">
          <EmptyState
            title={`no ${kind} found`}
            description={
              query
                ? "try a broader search, or publish a ready immutable release."
                : `create a ${kind === "models" ? "model" : "dataset"} repository and publish its first ready release.`
            }
          />
        </div>
      ) : (
        <ul className="divide-y divide-kumo-hairline border-x border-b border-kumo-hairline">
          {entries.map((entry) => {
            const repoName = entry.id.split("/").at(-1) ?? entry.id;
            return (
              <li key={`${kind}-${entry.id}`}>
                <Link
                  href={`/repos/${encodeURIComponent(repoName)}`}
                  className="block px-4 py-5 transition-colors hover:bg-kumo-elevated"
                >
                  <span className="flex flex-wrap items-start justify-between gap-3">
                    <span>
                      <span className="text-[0.9375rem]">{entry.id}</span>
                      <span className="mt-2 flex flex-wrap gap-1.5">
                        <Badge variant="outline">{kind === "models" ? "model" : "dataset"}</Badge>
                        <Badge variant="outline">{entry.private ? "private" : "public"}</Badge>
                        {entry.pipeline_tag && <Badge variant="outline">{entry.pipeline_tag}</Badge>}
                        {entry.library_name && <Badge variant="outline">{entry.library_name}</Badge>}
                        {(entry.clotho.evaluation_count ?? 0) > 0 && (
                          <Badge variant="outline">{entry.clotho.evaluation_count} evaluations</Badge>
                        )}
                        {entry.tags
                          .filter((tag) => ![kind.slice(0, -1), "clotho", "arachne"].includes(tag))
                          .slice(0, 4)
                          .map((tag) => (
                            <Badge key={tag} variant="outline">{tag}</Badge>
                          ))}
                      </span>
                    </span>
                    <span className="text-right text-[0.75rem] text-kumo-inactive">
                      <span className="block">{entry.clotho.release}</span>
                      <span className="mt-1 block">
                        {formatBytes(entry.usedStorage)} · {timeAgo(Date.parse(entry.lastModified))}
                      </span>
                    </span>
                  </span>
                  <span className="mt-3 block font-mono text-[0.6875rem] text-kumo-inactive">
                    {entry.sha.slice(0, 12)} · manifest {entry.clotho.manifest_sha256.slice(0, 12)} · Clotho source of truth
                  </span>
                </Link>
              </li>
            );
          })}
        </ul>
      )}
    </PageFrame>
  );
}
