import { Badge } from "@cloudflare/kumo";

import { api, timeAgo } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";

export const dynamic = "force-dynamic";

export default async function InsightsPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const client = api();
  const [commits, opLog, pulls, issues] = await Promise.all([
    client.commits(name, { limit: 50 }),
    client.opLog(name, 50),
    client.pulls(name, "all").catch(() => []),
    client.issues(name, "all").catch(() => []),
  ]);

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="insights" />
      <div className="mt-6">
        <h1 className="text-2xl leading-tight">insights</h1>
        <p className="mt-2 text-sm text-kumo-subtle">
          lightweight repository health from the clotho graph and facade.
        </p>
      </div>
      <div className="mt-8 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Metric label="commits" value={commits.length} />
        <Metric label="op log entries" value={opLog.length} />
        <Metric label="pull requests" value={pulls.length} />
        <Metric label="issues" value={issues.length} />
      </div>
      <section className="mt-8 border border-kumo-hairline p-4">
        <h2 className="text-sm">recent operations</h2>
        <ul className="mt-4 space-y-2">
          {opLog.slice(0, 12).map((op) => (
            <li key={op.operation_id} className="text-xs">
              <span>{op.description}</span>{" "}
              <span className="text-kumo-inactive">
                {timeAgo(op.end_time_millis)}
              </span>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <div className="border border-kumo-hairline p-4">
      <div className="text-xs text-kumo-inactive">{label}</div>
      <div className="mt-2 flex items-center gap-2">
        <span className="text-xl">{value}</span>
        <Badge variant="outline">live</Badge>
      </div>
    </div>
  );
}
