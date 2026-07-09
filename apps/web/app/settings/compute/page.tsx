import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import type { ComputeProvider } from "@clotho/sdk-js";

import { api } from "src/lib/api";

export const dynamic = "force-dynamic";

export default async function ComputeSettingsPage() {
  let providers: ComputeProvider[] = [];
  let defaultProviderId = "";
  let error: string | null = null;

  try {
    const list = await api().computeProviderList();
    providers = list.providers;
    defaultProviderId = list.default_provider_id;
  } catch (e) {
    error = e instanceof Error ? e.message : "failed to load providers";
    try {
      providers = await api().computeProviders();
    } catch {
      providers = [];
    }
  }

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <p className="text-xs text-kumo-inactive">
            <Link href="/" className="hover:text-kumo-default">
              dashboard
            </Link>
            <span className="mx-2">/</span>
            settings
          </p>
          <h1 className="mt-2 text-2xl leading-tight">compute providers</h1>
          <p className="mt-2 max-w-2xl text-sm text-kumo-subtle">
            Capability-aware CCI registry (Stage 12). Secrets stay in service
            environment variables and are never returned to the browser.
          </p>
        </div>
        {defaultProviderId && (
          <Badge variant="outline">default: {defaultProviderId}</Badge>
        )}
      </div>

      {error && (
        <p className="mt-6 border border-kumo-hairline px-4 py-3 text-xs text-kumo-subtle">
          {error}. showing empty or partial list.
        </p>
      )}

      {providers.length === 0 ? (
        <p className="mt-10 border border-kumo-hairline px-4 py-6 text-sm text-kumo-inactive">
          no providers reported. is the api gateway and clotho-compute up?
        </p>
      ) : (
        <ul className="mt-8 space-y-4">
          {providers.map((p) => (
            <li
              key={p.id}
              className="border border-kumo-hairline p-4 max-w-3xl"
            >
              <div className="flex flex-wrap items-center gap-2">
                <h2 className="text-sm">{p.name}</h2>
                <Badge variant="outline">{p.id}</Badge>
                {p.kind && <Badge variant="outline">{p.kind}</Badge>}
                <Badge variant="outline">
                  {p.configured ? "configured" : "not configured"}
                </Badge>
                {p.enabled && <Badge variant="outline">default</Badge>}
              </div>
              <dl className="mt-4 grid gap-2 text-xs">
                <Row
                  label="configured reason"
                  value={
                    p.configured
                      ? "ready"
                      : p.configured_reason || "credentials missing"
                  }
                />
                <Row
                  label="default snapshot"
                  value={p.default_snapshot || "provider default"}
                />
                <Row
                  label="capabilities"
                  value={(p.capabilities ?? []).join(", ") || "none"}
                />
                {p.capability_detail?.regions &&
                  p.capability_detail.regions.length > 0 && (
                    <Row
                      label="regions"
                      value={p.capability_detail.regions.join(", ")}
                    />
                  )}
                {p.capability_detail?.cost_hints && (
                  <Row
                    label="cost hints"
                    value={p.capability_detail.cost_hints}
                  />
                )}
                {p.notes && <Row label="notes" value={p.notes} />}
              </dl>
              {p.capability_detail && (
                <div className="mt-4 flex flex-wrap gap-1.5">
                  {capChip("one-shot", p.capability_detail.one_shot_jobs)}
                  {capChip(
                    "persistent",
                    p.capability_detail.persistent_workspaces,
                  )}
                  {capChip("snapshots", p.capability_detail.snapshots)}
                  {capChip("ssh", p.capability_detail.ssh)}
                  {capChip("desktop", p.capability_detail.desktop)}
                  {capChip("public url", p.capability_detail.public_url)}
                  {capChip("file api", p.capability_detail.file_api)}
                  {capChip(
                    "terminal stream",
                    p.capability_detail.terminal_streaming,
                  )}
                </div>
              )}
            </li>
          ))}
        </ul>
      )}

      <p className="mt-8 max-w-3xl text-xs text-kumo-inactive">
        Daytona uses <code className="text-kumo-subtle">DAYTONA_API_KEY</code>.
        ComputeSDK bridge uses{" "}
        <code className="text-kumo-subtle">CLOTHO_COMPUTE_SDK_BRIDGE_URL</code>{" "}
        and upstream keys in the sidecar. Box uses{" "}
        <code className="text-kumo-subtle">BOX_API_KEY</code> (stub until the
        full adapter lands). See{" "}
        <code className="text-kumo-subtle">.env.example</code>.
      </p>
    </div>
  );
}

function capChip(label: string, on: boolean) {
  return (
    <span
      className={`border px-2 py-0.5 text-[11px] ${
        on
          ? "border-kumo-hairline text-kumo-default"
          : "border-kumo-hairline text-kumo-inactive opacity-50"
      }`}
    >
      {label}
      {on ? "" : " —"}
    </span>
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
