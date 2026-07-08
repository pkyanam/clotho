import { Badge } from "@cloudflare/kumo";

import { api, shortId, timeAgo } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";

export const dynamic = "force-dynamic";

export default async function ChecksPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const client = api();
  const detail = await client.getRepo(name);
  const statuses = detail.main_commit_id
    ? await client.commitStatuses(name, detail.main_commit_id).catch(() => [])
    : [];

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="checks" />
      <div className="mt-6">
        <h1 className="text-2xl leading-tight">checks</h1>
        <p className="mt-2 text-sm text-kumo-subtle">
          commit statuses reported through clotho ci and provider integrations.
        </p>
      </div>
      <div className="mt-6 border border-kumo-hairline p-4 text-xs">
        main {detail.main_commit_id ? shortId(detail.main_commit_id) : "unborn"}
      </div>
      {statuses.length === 0 ? (
        <p className="mt-6 border border-kumo-hairline px-4 py-8 text-sm text-kumo-inactive">
          no statuses have been reported for main.
        </p>
      ) : (
        <ul className="mt-6 divide-y divide-kumo-hairline border border-kumo-hairline">
          {statuses.map((status) => (
            <li key={status.id} className="px-4 py-3">
              <div className="flex items-center justify-between gap-3">
                <span className="text-sm">{status.context || "status"}</span>
                <Badge variant="outline">{status.state}</Badge>
              </div>
              <p className="mt-1 text-xs text-kumo-inactive">
                {status.description || "no description"} ·{" "}
                {timeAgo(Date.parse(status.updated_at))}
              </p>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
