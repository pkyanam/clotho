import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";
import {
  ClothoApiError,
  type ActionsConfig,
  type ComputeProvider,
  type OrgDetail,
  type SecretMeta,
} from "@clotho/sdk-js";

import { api, publicCloneUrl, shortId } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import {
  MetaRow,
  PageFrame,
  SettingsSection,
} from "src/components/ui/page-frame";
import { GeneralForm } from "./general-form";
import { DeleteForm } from "./delete-form";
import { MergePolicyForm } from "./merge-policy-form";
import { createRepoSecret, deleteRepoSecret } from "./actions";

export const dynamic = "force-dynamic";

export default async function SettingsPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const client = await api();
  const [detail, actionsConfig, providerList, repoSecrets, mergePolicy] = await Promise.all([
    client.getRepo(name),
    client.actionsConfig(name).catch((e) => fallbackActionsConfig(e)),
    client.computeProviderList().catch(async () => {
      const providers = await client
        .computeProviders()
        .catch(() => [] as ComputeProvider[]);
      return { providers, default_provider_id: "" };
    }),
    client.repoSecrets(name).catch(() => [] as SecretMeta[]),
    client.getMergePolicy(name).catch(() => ({
      require_passing_actions: false,
      block_merge_when_conflicted: true,
      require_review_approvals: 0,
      protect_default_branch: false,
      updated_at: "",
    })),
  ]);
  const ownerOrg = detail.owner_org || detail.owner;
  const orgDetail: OrgDetail | null = ownerOrg
    ? await client.getOrg(ownerOrg).catch(() => null)
    : null;
  const providers = providerList.providers;
  const provider = providers.find((p) => p.id === actionsConfig.provider);
  const defaultProviderId =
    providerList.default_provider_id || actionsConfig.provider;
  const clone = publicCloneUrl(detail.clone_url, detail.owner, name);
  const description = detail.description || "";
  const createSecret = createRepoSecret.bind(null, name);

  return (
    <PageFrame>
      <RepoNav name={name} active="settings" />

      <div className="mt-6 border-b border-kumo-hairline pb-6">
        <h1
          className="leading-tight text-kumo-default"
          style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
        >
          settings
        </h1>
        <p className="mt-2 text-[0.9375rem] leading-relaxed text-kumo-inactive">
          general, access, secrets, actions, and compute for this repository.
        </p>
      </div>

      <nav className="mt-6 flex flex-wrap gap-2 text-[0.8125rem]">
        {[
          ["#general", "general"],
          ["#merge", "merge"],
          ["#collaborators", "collaborators"],
          ["#secrets", "secrets"],
          ["#actions", "actions"],
          ["#compute", "compute"],
          ["#danger", "danger zone"],
        ].map(([href, label]) => (
          <a
            key={href}
            href={href}
            className="border border-kumo-hairline px-3 py-1.5 text-kumo-inactive transition-colors hover:bg-kumo-elevated hover:text-kumo-default"
          >
            {label}
          </a>
        ))}
      </nav>

      <div className="mt-8 max-w-3xl space-y-6">
        <div id="general">
          <SettingsSection title="general" description="repository metadata.">
            <GeneralForm
              repo={name}
              description={description}
              visibility={detail.visibility}
              defaultBranch={detail.default_branch}
              kind={detail.kind}
              largeFileThresholdBytes={detail.large_file_threshold_bytes}
              networkMode={detail.network_mode}
              networkTags={detail.network_tags}
            />
            <dl className="mt-6 border-t border-kumo-hairline pt-4">
              <MetaRow label="owner" value={ownerOrg} />
              <MetaRow label="visibility" value={detail.visibility} />
              <MetaRow label="kind" value={detail.kind} />
              <MetaRow label="network" value={detail.network_mode} />
              <MetaRow label="default branch" value={detail.default_branch} />
              <MetaRow
                label="clone url"
                value={<code className="text-kumo-default">{clone}</code>}
              />
              <MetaRow
                label="main commit"
                value={
                  detail.main_commit_id
                    ? shortId(detail.main_commit_id)
                    : "unborn — no commits yet"
                }
              />
            </dl>
          </SettingsSection>
        </div>

        <div id="merge">
          <SettingsSection
            title="merge policy"
            description="clotho-owned gates enforced when merging pull requests."
          >
            <MergePolicyForm repo={name} policy={mergePolicy} />
          </SettingsSection>
        </div>

        <div id="collaborators">
          <SettingsSection
            title="collaborators"
            description="access is granted through the owning organization. everyone below can read and write this repository."
            badge={
              orgDetail ? (
                <Badge variant="outline">{orgDetail.members.length}</Badge>
              ) : undefined
            }
          >
            {orgDetail && orgDetail.members.length > 0 ? (
              <>
                <ul className="divide-y divide-kumo-hairline border border-kumo-hairline">
                  {orgDetail.members.map((m) => (
                    <li
                      key={m.user_id}
                      className="flex items-center justify-between px-3 py-2.5 text-[0.875rem]"
                    >
                      <span className="text-kumo-default">
                        {m.user_display_name || m.user_name}
                      </span>
                      <span className="text-[0.8125rem] text-kumo-inactive">
                        {m.role}
                      </span>
                    </li>
                  ))}
                </ul>
                <p className="mt-4 text-[0.8125rem] text-kumo-inactive">
                  manage membership in{" "}
                  <Link
                    href={`/orgs/${ownerOrg}`}
                    className="underline hover:text-kumo-default"
                  >
                    the organization
                  </Link>
                  . per-repository roles are on the roadmap.
                </p>
              </>
            ) : (
              <p className="text-[0.875rem] text-kumo-inactive">
                membership is managed in{" "}
                <Link
                  href={`/orgs/${ownerOrg}`}
                  className="underline hover:text-kumo-default"
                >
                  the owning organization
                </Link>
                . per-repository roles are on the roadmap.
              </p>
            )}
          </SettingsSection>
        </div>

        <div id="secrets">
          <SettingsSection
            title="repository secrets"
            description="scoped tokens and deploy keys for this repository. values are write-only — never shown again after save."
            badge={<Badge variant="outline">{repoSecrets.length}</Badge>}
          >
            {repoSecrets.length === 0 ? (
              <p className="text-[0.875rem] text-kumo-inactive">
                no repository secrets yet. shared credentials live in{" "}
                <Link
                  href="/settings/secrets"
                  className="underline hover:text-kumo-default"
                >
                  organization secrets
                </Link>
                .
              </p>
            ) : (
              <ul className="divide-y divide-kumo-hairline border border-kumo-hairline">
                {repoSecrets.map((s) => (
                  <li
                    key={s.id}
                    className="flex flex-wrap items-center justify-between gap-3 px-3 py-2 text-[0.875rem]"
                  >
                    <span>
                      {s.name}
                      {s.value_last4 && (
                        <span className="ml-2 text-kumo-inactive">
                          ···{s.value_last4}
                        </span>
                      )}
                    </span>
                    <form action={deleteRepoSecret.bind(null, name, s.name)}>
                      <Button type="submit" variant="outline">
                        delete
                      </Button>
                    </form>
                  </li>
                ))}
              </ul>
            )}
            <div className="mt-6 border border-kumo-hairline bg-kumo-base p-5">
              <h3 className="text-[0.9375rem]">add repository secret</h3>
              <p className="mt-2 text-[0.8125rem] leading-relaxed text-kumo-inactive">
                the value is encrypted at rest and never returned after save.
              </p>
              <form action={createSecret} className="mt-5 space-y-4">
                <label className="block text-[0.8125rem] text-kumo-inactive">
                  name
                  <input
                    name="name"
                    required
                    pattern="[A-Za-z0-9_-]+"
                    placeholder="DEPLOY_KEY"
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
                    ? "provider connected"
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
            description="destructive operations are deliberate, audited, and confirmed — never one accidental click."
          >
            <div className="space-y-3">
              <DangerRow
                title="transfer repository"
                body="transfer is not available yet — contact your admin to move ownership."
                action="transfer"
                disabled
              />
              <div className="border border-kumo-hairline px-4 py-3">
                <div className="min-w-0 max-w-xl">
                  <p className="text-[0.875rem] text-kumo-default">
                    delete repository
                  </p>
                  <p className="mt-1 text-[0.8125rem] leading-relaxed text-kumo-inactive">
                    permanently remove this repository and its collaboration
                    threads. vcs history may remain until a full purge ships.
                  </p>
                </div>
                <div className="mt-4">
                  <DeleteForm repo={name} />
                </div>
              </div>
            </div>
          </SettingsSection>
        </div>
      </div>
    </PageFrame>
  );
}

function DangerRow({
  title,
  body,
  action,
  disabled = true,
}: {
  title: string;
  body: string;
  action: string;
  disabled?: boolean;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 border border-kumo-hairline px-4 py-3">
      <div className="min-w-0 max-w-xl">
        <p className="text-[0.875rem] text-kumo-default">{title}</p>
        <p className="mt-1 text-[0.8125rem] leading-relaxed text-kumo-inactive">
          {body}
        </p>
      </div>
      <Button type="button" variant="outline" disabled={disabled}>
        {action}
      </Button>
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
