import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { ClothoApiError, type ActionsConfig, type ComputeProvider } from "@clotho/sdk-js";

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
  const [detail, actionsConfig, providerList] = await Promise.all([
    client.getRepo(name),
    client.actionsConfig(name).catch((e) => fallbackActionsConfig(e)),
    client.computeProviderList().catch(async () => {
      const providers = await client.computeProviders().catch(() => [] as ComputeProvider[]);
      return { providers, default_provider_id: "" };
    }),
  ]);
  const providers = providerList.providers;
  const provider = providers.find((p) => p.id === actionsConfig.provider);
  const defaultProviderId =
    providerList.default_provider_id || actionsConfig.provider;

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="settings" />
      <div className="mt-6">
        <h1 className="text-2xl leading-tight">settings</h1>
        <p className="mt-2 text-sm text-kumo-subtle">
          repository metadata, actions defaults, and compute providers.
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
          forgejo is kept behind the api gateway. primary workflows stay in
          clotho.
        </p>
      </section>
      <section className="mt-6 max-w-3xl border border-kumo-hairline p-4">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-sm">actions defaults</h2>
          <Badge variant="outline">
            {actionsConfig.enabled ? "enabled" : "disabled"}
          </Badge>
          <Badge variant="outline">
            {provider?.configured ? "provider configured" : "provider not configured"}
          </Badge>
        </div>
        <dl className="mt-4 grid gap-3 text-xs">
          <Row
            label="provider"
            value={provider?.name ?? detail.provider ?? actionsConfig.provider}
          />
          <Row
            label="platform default"
            value={defaultProviderId || "—"}
          />
          <Row
            label="compute configured"
            value={detail.configured ? "yes" : "no"}
          />
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
            ? "actions route through the CCI registry by provider id — callers never hard-code daytona. secrets stay in the service environment."
            : "actions config is not available from the api gateway currently serving this page."}
        </p>
      </section>
      <section className="mt-6 max-w-3xl border border-kumo-hairline p-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-sm">compute providers</h2>
          <Link
            href="/settings/compute"
            className="text-xs text-kumo-subtle hover:text-kumo-default"
          >
            full registry →
          </Link>
        </div>
        {providers.length === 0 ? (
          <p className="mt-3 text-xs text-kumo-inactive">
            no providers listed (gateway or compute unreachable).
          </p>
        ) : (
          <ul className="mt-4 divide-y divide-kumo-hairline border border-kumo-hairline">
            {providers.map((p) => (
              <li
                key={p.id}
                className="flex flex-wrap items-center justify-between gap-2 px-3 py-2 text-xs"
              >
                <span className="flex flex-wrap items-center gap-2">
                  <span>{p.name}</span>
                  <span className="text-kumo-inactive">{p.id}</span>
                  {p.kind && (
                    <Badge variant="outline">{p.kind}</Badge>
                  )}
                </span>
                <span className="flex flex-wrap items-center gap-2">
                  {p.enabled && <Badge variant="outline">default</Badge>}
                  <Badge variant="outline">
                    {p.configured ? "configured" : "not configured"}
                  </Badge>
                </span>
              </li>
            ))}
          </ul>
        )}
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
