import { Badge } from "@cloudflare/kumo";
import { ClothoApiError, type ActionsConfig } from "@clotho/sdk-js";

import { api } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";

export const dynamic = "force-dynamic";

export default async function SettingsPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const client = api();
  const [detail, actionsConfig, providers] = await Promise.all([
    client.getRepo(name),
    client.actionsConfig(name).catch((e) => fallbackActionsConfig(e)),
    client.computeProviders().catch(() => []),
  ]);
  const provider = providers.find((p) => p.id === actionsConfig.provider);

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
          <Row label="owner" value={detail.owner_org || detail.owner} />
          <Row label="clone owner" value={detail.owner} />
          <Row label="visibility" value={detail.visibility} />
          <Row label="default branch" value={detail.default_branch} />
          <Row label="clone url" value={detail.clone_url} />
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
      <section className="mt-6 max-w-3xl border border-kumo-hairline p-4">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-sm">actions runner</h2>
          <Badge variant="outline">{actionsConfig.enabled ? "enabled" : "disabled"}</Badge>
          <Badge variant="outline">
            {provider?.configured ? "configured" : "not configured"}
          </Badge>
        </div>
        <dl className="mt-4 grid gap-3 text-xs">
          <Row label="provider" value={provider?.name ?? detail.provider ?? actionsConfig.provider} />
          <Row label="compute configured" value={detail.configured ? "yes" : "no"} />
          <Row
            label="default image"
            value={actionsConfig.default_image || "provider default"}
          />
          <Row
            label="timeout"
            value={`${actionsConfig.timeout_seconds} seconds`}
          />
          <Row
            label="capabilities"
            value={(provider?.capabilities ?? []).join(", ") || "unknown"}
          />
        </dl>
        <p className="mt-4 text-xs text-kumo-inactive">
          {actionsConfig.enabled
            ? "secret values are read from service environment and are never returned to the browser."
            : "actions config is not available from the api gateway currently serving this page."}
        </p>
      </section>
    </div>
  );
}

function fallbackActionsConfig(error: unknown): ActionsConfig {
  if (error instanceof ClothoApiError && error.status === 404) {
    return {
      enabled: false,
      provider: "daytona",
      default_image: "ubuntu:22.04",
      timeout_seconds: 900,
    };
  }
  throw error;
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[160px_minmax(0,1fr)]">
      <dt className="text-kumo-inactive">{label}</dt>
      <dd className="min-w-0 break-all">{value}</dd>
    </div>
  );
}
