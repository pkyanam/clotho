import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import {
  ClothoApiError,
  type ActionsConfig,
  type ComputeProvider,
  type SecretMeta,
} from "@clotho/sdk-js";

import { api, publicCloneUrl } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import {
  MetaRow,
  PageFrame,
  SettingsSection,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function SettingsPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const client = api();
  const [detail, actionsConfig, providerList, repoSecrets] = await Promise.all([
    client.getRepo(name),
    client.actionsConfig(name).catch((e) => fallbackActionsConfig(e)),
    client.computeProviderList().catch(async () => {
      const providers = await client
        .computeProviders()
        .catch(() => [] as ComputeProvider[]);
      return { providers, default_provider_id: "" };
    }),
    client.repoSecrets(name).catch(() => [] as SecretMeta[]),
  ]);
  const providers = providerList.providers;
  const provider = providers.find((p) => p.id === actionsConfig.provider);
  const defaultProviderId =
    providerList.default_provider_id || actionsConfig.provider;
  const clone = publicCloneUrl(detail.clone_url, detail.owner, name);

  return (
    <PageFrame>
      <RepoNav name={name} active="settings" />

      <div className="mt-6">
        <h1
          className="leading-tight"
          style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
        >
          settings
        </h1>
        <p className="mt-2 text-[0.9375rem] text-kumo-inactive">
          general, secrets, actions, and compute for this repository.
        </p>
      </div>

      <nav className="mt-6 flex flex-wrap gap-2 text-[0.8125rem]">
        {[
          ["#general", "general"],
          ["#collaborators", "collaborators"],
          ["#secrets", "secrets"],
          ["#actions", "actions"],
          ["#compute", "compute"],
          ["#danger", "danger zone"],
        ].map(([href, label]) => (
          <a
            key={href}
            href={href}
            className="border border-kumo-hairline px-3 py-1.5 text-kumo-inactive hover:text-kumo-default"
          >
            {label}
          </a>
        ))}
      </nav>

      <div className="mt-8 max-w-3xl space-y-6">
        <div id="general">
          <SettingsSection title="general" description="repository metadata.">
            <dl>
              <MetaRow
                label="owner"
                value={detail.owner_org || detail.owner}
              />
              <MetaRow label="visibility" value={detail.visibility} />
              <MetaRow label="default branch" value={detail.default_branch} />
              <MetaRow label="clone url" value={clone} />
              <MetaRow
                label="main commit"
                value={detail.main_commit_id || "unborn"}
              />
            </dl>
          </SettingsSection>
        </div>

        <div id="collaborators">
          <SettingsSection
            title="collaborators"
            description="membership is managed at the organization level for now."
          >
            <p className="text-[0.875rem] text-kumo-inactive">
              open{" "}
              <Link
                href={`/orgs/${detail.owner_org || detail.owner}`}
                className="underline hover:text-kumo-default"
              >
                organization settings
              </Link>{" "}
              to manage members. fine-grained repo roles land in a later stage.
            </p>
          </SettingsSection>
        </div>

        <div id="secrets">
          <SettingsSection
            title="repository secrets"
            description="ci-only tokens and deploy keys. values are write-only."
            badge={<Badge variant="outline">{repoSecrets.length}</Badge>}
          >
            {repoSecrets.length === 0 ? (
              <p className="text-[0.875rem] text-kumo-inactive">
                no repository secrets. add org secrets under{" "}
                <Link
                  href="/settings/secrets"
                  className="underline hover:text-kumo-default"
                >
                  settings → secrets
                </Link>
                , or use the api / sdk to create repo-scoped secrets.
              </p>
            ) : (
              <ul className="divide-y divide-kumo-hairline border border-kumo-hairline">
                {repoSecrets.map((s) => (
                  <li
                    key={s.id}
                    className="flex items-center justify-between px-3 py-2 text-[0.875rem]"
                  >
                    <span>{s.name}</span>
                    <span className="text-kumo-inactive">
                      {s.value_last4 ? `···${s.value_last4}` : "configured"}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </SettingsSection>
        </div>

        <div id="actions">
          <SettingsSection
            title="actions"
            description="defaults for sandbox runs on this repository."
            badge={
              <>
                <Badge variant="outline">
                  {actionsConfig.enabled ? "enabled" : "disabled"}
                </Badge>
                <Badge variant="outline">
                  {provider?.configured
                    ? "provider configured"
                    : "provider not connected"}
                </Badge>
              </>
            }
          >
            <dl>
              <MetaRow
                label="provider"
                value={
                  provider?.name ?? detail.provider ?? actionsConfig.provider
                }
              />
              <MetaRow
                label="platform default"
                value={defaultProviderId || "—"}
              />
              <MetaRow
                label="default image"
                value={actionsConfig.default_image || "provider default"}
              />
              <MetaRow
                label="timeout"
                value={`${actionsConfig.timeout_seconds} seconds`}
              />
            </dl>
            {!provider?.configured && (
              <p className="mt-4 text-[0.8125rem] text-kumo-inactive">
                connect a compute provider in{" "}
                <Link
                  href="/settings/compute"
                  className="underline hover:text-kumo-default"
                >
                  settings → compute
                </Link>{" "}
                to run actions.
              </p>
            )}
          </SettingsSection>
        </div>

        <div id="compute">
          <SettingsSection
            title="compute providers"
            description="registry state for this workspace."
            badge={
              <Link
                href="/settings/compute"
                className="text-[0.8125rem] text-kumo-inactive hover:text-kumo-default"
              >
                full registry →
              </Link>
            }
          >
            {providers.length === 0 ? (
              <p className="text-[0.875rem] text-kumo-inactive">
                no providers listed.
              </p>
            ) : (
              <ul className="divide-y divide-kumo-hairline border border-kumo-hairline">
                {providers.map((p) => (
                  <li
                    key={p.id}
                    className="flex flex-wrap items-center justify-between gap-2 px-3 py-2.5 text-[0.875rem]"
                  >
                    <span className="flex flex-wrap items-center gap-2">
                      <span>{p.name}</span>
                      <span className="text-kumo-inactive">{p.id}</span>
                    </span>
                    <span className="flex flex-wrap items-center gap-2">
                      {p.enabled && <Badge variant="outline">default</Badge>}
                      <Badge variant="outline">
                        {p.configured ? "configured" : "not connected"}
                      </Badge>
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </SettingsSection>
        </div>

        <div id="danger">
          <SettingsSection
            title="danger zone"
            description="destructive operations. not yet available in this slice."
          >
            <p className="text-[0.875rem] text-kumo-inactive">
              repository transfer and deletion will appear here with confirm
              dialogs in a later stage.
            </p>
          </SettingsSection>
        </div>
      </div>
    </PageFrame>
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
