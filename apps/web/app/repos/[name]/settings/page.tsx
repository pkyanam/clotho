import { Badge } from "@cloudflare/kumo";

import { api, cloneUrl } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";

export const dynamic = "force-dynamic";

export default async function SettingsPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const detail = await api().getRepo(name);

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="settings" />
      <div className="mt-6">
        <h1 className="text-2xl leading-tight">settings</h1>
        <p className="mt-2 text-sm text-kumo-subtle">
          repository metadata and integration endpoints exposed by clotho.
        </p>
      </div>
      <section className="mt-8 max-w-3xl border border-kumo-hairline p-4">
        <h2 className="text-sm">repository</h2>
        <dl className="mt-4 grid gap-3 text-xs">
          <Row label="owner" value={detail.owner} />
          <Row label="default branch" value={detail.forgejo.default_branch} />
          <Row label="clone url" value={cloneUrl(detail.owner, name)} />
          <Row label="main commit" value={detail.main_commit_id || "unborn"} />
        </dl>
      </section>
      <section className="mt-6 max-w-3xl border border-kumo-hairline p-4">
        <div className="flex items-center gap-2">
          <h2 className="text-sm">collaboration provider</h2>
          <Badge variant="outline">internal</Badge>
        </div>
        <p className="mt-3 text-xs text-kumo-inactive">
          forgejo is kept behind the api gateway for stage 9. primary workflows
          stay in clotho.
        </p>
      </section>
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[160px_minmax(0,1fr)]">
      <dt className="text-kumo-inactive">{label}</dt>
      <dd className="min-w-0 break-all">{value}</dd>
    </div>
  );
}
