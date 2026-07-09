import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";
import type { SecretMeta } from "@clotho/sdk-js";

import { api, timeAgo } from "src/lib/api";
import { SettingsNav } from "src/components/settings-nav";
import {
  EmptyState,
  PageFrame,
  PageTitle,
} from "src/components/ui/page-frame";
import { createOrgSecret, deleteOrgSecret } from "./actions";

export const dynamic = "force-dynamic";

function isoToMillis(iso: string): number {
  const t = Date.parse(iso);
  return Number.isFinite(t) ? t : 0;
}

export default async function SecretsSettingsPage() {
  const orgs = await api()
    .orgs()
    .catch(() => []);
  const primaryOrg = orgs[0]?.name ?? "clotho";

  let secrets: SecretMeta[] = [];
  let loadError: string | null = null;
  try {
    secrets = await api().orgSecrets(primaryOrg);
  } catch (e) {
    loadError = e instanceof Error ? e.message : "failed to load secrets";
  }

  const create = createOrgSecret.bind(null, primaryOrg);

  return (
    <PageFrame>
      <PageTitle
        title="secrets"
        description="organization secrets for compute providers and Actions. values are write-only — never shown again after save."
        eyebrow={<SettingsNav active="secrets" />}
      />

      <div className="mt-8 grid gap-8 lg:grid-cols-[minmax(0,1fr)_340px]">
        <section>
          <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
            <h2 className="text-[0.9375rem]">
              org secrets
              <span className="ml-2 text-[0.8125rem] text-kumo-inactive">
                {primaryOrg}
              </span>
            </h2>
            <Badge variant="outline">{secrets.length} secrets</Badge>
          </div>

          {loadError ? (
            <EmptyState
              title="could not load secrets"
              description={loadError}
            />
          ) : secrets.length === 0 ? (
            <EmptyState
              title="no secrets yet"
              description="add an organization secret, or connect a compute provider — the key is stored here automatically."
              action={
                <Link href="/settings/compute">
                  <Button type="button" variant="outline">
                    connect compute
                  </Button>
                </Link>
              }
            />
          ) : (
            <ul className="divide-y divide-kumo-hairline border border-kumo-hairline">
              {secrets.map((s) => (
                <li
                  key={s.id}
                  className="flex flex-wrap items-center justify-between gap-3 px-4 py-3.5"
                >
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-medium text-[0.9375rem]">
                        {s.name}
                      </span>
                      <Badge variant="outline">{s.scope}</Badge>
                      {s.value_last4 && (
                        <span className="text-[0.8125rem] text-kumo-inactive">
                          ···{s.value_last4}
                        </span>
                      )}
                    </div>
                    {s.description && (
                      <p className="mt-1 text-[0.8125rem] text-kumo-inactive">
                        {s.description}
                      </p>
                    )}
                    <p className="mt-1 text-[0.75rem] text-kumo-inactive">
                      updated {timeAgo(isoToMillis(s.updated_at))}
                    </p>
                  </div>
                  <form action={deleteOrgSecret.bind(null, primaryOrg, s.name)}>
                    <Button type="submit" variant="outline">
                      delete
                    </Button>
                  </form>
                </li>
              ))}
            </ul>
          )}
        </section>

        <aside>
          <div className="border border-kumo-hairline bg-kumo-base p-5">
            <h2 className="text-[0.9375rem]">add secret</h2>
            <p className="mt-2 text-[0.8125rem] leading-relaxed text-kumo-inactive">
              the value is encrypted at rest and never returned to the browser
              after save. use names like{" "}
              <code className="text-kumo-default">DAYTONA_API_KEY</code> for
              provider bindings.
            </p>
            <form action={create} className="mt-5 space-y-4">
              <label className="block text-[0.8125rem] text-kumo-inactive">
                name
                <input
                  name="name"
                  required
                  pattern="[A-Za-z0-9_-]+"
                  placeholder="DAYTONA_API_KEY"
                  className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default outline-none focus:border-kumo-contrast"
                />
              </label>
              <label className="block text-[0.8125rem] text-kumo-inactive">
                value
                <input
                  name="value"
                  type="password"
                  required
                  autoComplete="off"
                  className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default outline-none focus:border-kumo-contrast"
                />
              </label>
              <label className="block text-[0.8125rem] text-kumo-inactive">
                description
                <input
                  name="description"
                  placeholder="optional"
                  className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default outline-none focus:border-kumo-contrast"
                />
              </label>
              <Button type="submit">save secret</Button>
            </form>
          </div>
        </aside>
      </div>
    </PageFrame>
  );
}
