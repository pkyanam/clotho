import { Badge, Button, Input } from "@cloudflare/kumo";
import type { FabricProvider, FabricProviderList } from "@clotho/sdk-js";

import { api } from "src/lib/api";
import { SettingsNav } from "src/components/settings-nav";
import {
  PageFrame,
  PageTitle,
  SettingsSection,
} from "src/components/ui/page-frame";
import { connectTailscale, disconnectTailscale } from "./actions";

export const dynamic = "force-dynamic";

export default async function NetworkSettingsPage() {
  const client = await api();
  const orgs = await client.orgs().catch(() => []);
  const org =
    orgs.find((candidate) => candidate.name === "clotho")?.name ??
    orgs[0]?.name ??
    "clotho";
  const list = (await client
    .listProviders({ layer: "network", org })
    .catch(() => ({ providers: [], default_provider_id: "public" }))) as FabricProviderList;
  const tailscale = list.providers.find((provider) => provider.id === "tailscale");

  return (
    <PageFrame>
      <PageTitle
        title="network"
        description="private reach is an organization and repository capability — not workflow glue."
        eyebrow={<SettingsNav active="network" />}
        actions={
          <Badge variant="outline">
            {tailscale?.configured ? "Tailscale connected" : "public network"}
          </Badge>
        }
      />

      <div className="mt-8 grid gap-6 lg:grid-cols-2">
        <SettingsSection
          title="Tailscale"
          description="live-probed OAuth client for ephemeral tagged Actions, sandboxes, and BYOC runners."
        >
          <div className="p-5">
            <ProviderState provider={tailscale} />
            {tailscale?.configured ? (
              <form action={disconnectTailscale} className="mt-5">
                <input type="hidden" name="org" value={org} />
                <Button type="submit" variant="outline">disconnect Tailscale</Button>
              </form>
            ) : (
              <form action={connectTailscale} className="mt-5 space-y-4">
                <input type="hidden" name="org" value={org} />
                <label className="block text-[0.8125rem] text-kumo-inactive">
                  OAuth client id
                  <Input name="client_id" required autoComplete="off" className="mt-1 w-full" placeholder="k…" />
                </label>
                <label className="block text-[0.8125rem] text-kumo-inactive">
                  OAuth client secret
                  <Input name="client_secret" type="password" required autoComplete="new-password" className="mt-1 w-full" placeholder="tskey-client-…" />
                </label>
                <p className="text-[0.75rem] leading-relaxed text-kumo-inactive">
                  Create the client with <code>auth_keys</code> scope and the tag below. Clotho verifies it live before encrypting it; the secret is never returned.
                </p>
                <Button type="submit">connect and verify</Button>
              </form>
            )}
          </div>
        </SettingsSection>

        <SettingsSection
          title="tailnet policy helper"
          description="copy this into your Tailscale grants policy, then create the OAuth client for the same tag."
        >
          <div className="p-5">
            <p className="text-[0.8125rem] text-kumo-inactive">suggested identity</p>
            <code className="mt-2 block border border-kumo-hairline bg-kumo-base px-3 py-2 text-[0.8125rem] text-kumo-default">
              tag:clotho-{org}
            </code>
            <pre className="mt-4 overflow-x-auto border border-kumo-hairline bg-kumo-base p-4 text-[0.75rem] leading-relaxed text-kumo-default">{`{
  "tagOwners": {
    "tag:clotho-${org}": ["autogroup:admin"]
  },
  "grants": [{
    "src": ["tag:clotho-${org}"],
    "dst": ["tag:clotho-services"],
    "ip": ["*"]
  }]
}`}</pre>
            <p className="mt-3 text-[0.75rem] leading-relaxed text-kumo-inactive">
              Clotho does not rewrite your tailnet policy. Narrow the destination tag and ports to the private services each repository needs.
            </p>
          </div>
        </SettingsSection>
      </div>
    </PageFrame>
  );
}

function ProviderState({ provider }: { provider?: FabricProvider }) {
  if (!provider) {
    return <p className="text-[0.8125rem] text-kumo-inactive">network registry unavailable</p>;
  }
  return (
    <div>
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="outline">{provider.configured ? "verified" : "not connected"}</Badge>
        {provider.capabilities.map((capability) => (
          <Badge key={capability} variant="outline">{capability}</Badge>
        ))}
      </div>
      <p className="mt-3 text-[0.8125rem] leading-relaxed text-kumo-inactive">
        {provider.configured_reason}
      </p>
    </div>
  );
}
