import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";
import type { ComputeProvider } from "@clotho/sdk-js";

import { api } from "src/lib/api";
import { SettingsNav } from "src/components/settings-nav";
import {
  EmptyState,
  PageFrame,
  PageTitle,
} from "src/components/ui/page-frame";
import { connectProvider, disconnectProvider } from "./actions";

export const dynamic = "force-dynamic";

export default async function ComputeSettingsPage() {
  let providers: ComputeProvider[] = [];
  let defaultProviderId = "";
  let error: string | null = null;
  const orgs = await api()
    .orgs()
    .catch(() => []);
  const primaryOrg = orgs[0]?.name ?? "clotho";

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
    <PageFrame>
      <PageTitle
        title="compute"
        description="capability-aware provider registry. connect credentials in Clotho — no host env files required."
        eyebrow={<SettingsNav active="compute" />}
        actions={
          defaultProviderId ? (
            <Badge variant="outline">default: {defaultProviderId}</Badge>
          ) : undefined
        }
      />

      {error && (
        <p className="mt-6 border border-kumo-hairline px-4 py-3 text-[0.8125rem] text-kumo-inactive">
          {error}. showing empty or partial list.
        </p>
      )}

      {providers.length === 0 ? (
        <div className="mt-10">
          <EmptyState
            title="no providers reported"
            description="start the api gateway and compute service, then refresh."
          />
        </div>
      ) : (
        <ul className="mt-8 space-y-5">
          {providers.map((p) => (
            <li
              key={p.id}
              className="border border-kumo-hairline bg-kumo-base p-5 max-w-3xl"
            >
              <div className="flex flex-wrap items-center gap-2">
                <h2 className="text-[1rem]">{p.name}</h2>
                <Badge variant="outline">{p.id}</Badge>
                {p.kind && <Badge variant="outline">{p.kind}</Badge>}
                <Badge variant="outline">
                  {p.configured ? "configured" : "not connected"}
                </Badge>
                {p.enabled && <Badge variant="outline">default</Badge>}
              </div>

              <p className="mt-3 text-[0.875rem] text-kumo-inactive">
                {p.configured
                  ? p.configured_reason || "ready for Actions and sandboxes"
                  : p.configured_reason ||
                    "connect credentials below to enable this provider"}
              </p>

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

              <dl className="mt-4 grid gap-2 text-[0.8125rem]">
                <Row
                  label="default snapshot"
                  value={p.default_snapshot || "provider default"}
                />
                {p.capability_detail?.regions &&
                  p.capability_detail.regions.length > 0 && (
                    <Row
                      label="regions"
                      value={p.capability_detail.regions.join(", ")}
                    />
                  )}
                {p.capability_detail?.cost_hints && (
                  <Row label="cost" value={p.capability_detail.cost_hints} />
                )}
              </dl>

              {(p.id === "daytona" || p.id === "box") && (
                <div className="mt-5 border-t border-kumo-hairline pt-5">
                  <h3 className="text-[0.875rem]">
                    {p.configured ? "rotate connection" : `connect ${p.name}`}
                  </h3>
                  <p className="mt-1 text-[0.8125rem] text-kumo-inactive">
                    paste an api key once. it is stored as an organization
                    secret and never shown again.
                  </p>
                  <form
                    action={connectProvider.bind(null, p.id)}
                    className="mt-3 flex flex-wrap items-end gap-3"
                  >
                    <input type="hidden" name="org" value={primaryOrg} />
                    <label className="min-w-[220px] grow text-[0.8125rem] text-kumo-inactive">
                      api key
                      <input
                        name="api_key"
                        type="password"
                        required
                        autoComplete="off"
                        placeholder={
                          p.configured ? "enter new key to rotate" : "paste key"
                        }
                        className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default outline-none focus:border-kumo-contrast"
                      />
                    </label>
                    <Button type="submit">
                      {p.configured ? "rotate" : "connect"}
                    </Button>
                  </form>
                  {p.configured && (
                    <form
                      action={disconnectProvider.bind(null, p.id)}
                      className="mt-3"
                    >
                      <input type="hidden" name="org" value={primaryOrg} />
                      <Button type="submit" variant="outline">
                        disconnect
                      </Button>
                    </form>
                  )}
                </div>
              )}

              {p.id === "computesdk" && (
                <div className="mt-5 border-t border-kumo-hairline pt-5">
                  <h3 className="text-[0.875rem]">
                    {p.configured
                      ? "rotate ComputeSDK upstream key"
                      : "connect ComputeSDK upstream"}
                  </h3>
                  <p className="mt-1 text-[0.8125rem] text-kumo-inactive">
                    store an E2B (or Modal) key in Clotho. start the optional
                    bridge with{" "}
                    <code className="text-kumo-default">
                      just dev-compute-bridge
                    </code>{" "}
                    — no host env file required for keys. values are never shown
                    again.
                  </p>
                  <form
                    action={connectProvider.bind(null, p.id)}
                    className="mt-3 flex flex-wrap items-end gap-3"
                  >
                    <input type="hidden" name="org" value={primaryOrg} />
                    <input type="hidden" name="upstream" value="e2b" />
                    <label className="min-w-[220px] grow text-[0.8125rem] text-kumo-inactive">
                      E2B api key
                      <input
                        name="api_key"
                        type="password"
                        required
                        autoComplete="off"
                        placeholder={
                          p.configured
                            ? "enter new key to rotate"
                            : "paste E2B key"
                        }
                        className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default outline-none focus:border-kumo-contrast"
                      />
                    </label>
                    <Button type="submit">
                      {p.configured ? "rotate" : "connect E2B"}
                    </Button>
                  </form>
                  {p.configured && (
                    <form
                      action={disconnectProvider.bind(null, p.id)}
                      className="mt-3"
                    >
                      <input type="hidden" name="org" value={primaryOrg} />
                      <Button type="submit" variant="outline">
                        disconnect
                      </Button>
                    </form>
                  )}
                </div>
              )}
            </li>
          ))}
        </ul>
      )}

      <p className="mt-8 max-w-3xl text-[0.8125rem] text-kumo-inactive">
        manage all secrets in{" "}
        <Link
          href="/settings/secrets"
          className="underline hover:text-kumo-default"
        >
          settings → secrets
        </Link>
        . advanced bootstrap (master key, compose profiles) is documented in{" "}
        <code className="text-kumo-default">.env.example</code> and the repo
        docs — not required for day-to-day provider connect.
      </p>
    </PageFrame>
  );
}

function capChip(label: string, on: boolean) {
  return (
    <span
      className={`border px-2 py-0.5 text-[0.75rem] ${
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
    <div className="grid gap-1 sm:grid-cols-[140px_minmax(0,1fr)]">
      <dt className="text-kumo-inactive">{label}</dt>
      <dd className="min-w-0 break-all">{value}</dd>
    </div>
  );
}
