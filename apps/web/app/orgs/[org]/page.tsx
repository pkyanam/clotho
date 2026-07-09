import { Badge } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api } from "src/lib/api";
import {
  EmptyState,
  PageFrame,
  PageTitle,
  SectionHeader,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function OrgDetailPage({
  params,
}: {
  params: Promise<{ org: string }>;
}) {
  const { org } = await params;
  const client = api();
  const detail = await client.getOrg(org).catch((e) => {
    if (e instanceof ClothoApiError && e.status === 404) notFound();
    throw e;
  });
  const repos = await client.getOrgRepos(org).catch(() => []);

  return (
    <PageFrame>
      <PageTitle
        title={detail.org.display_name || detail.org.name}
        description={`organization · ${detail.org.name}`}
        eyebrow={
          <Link
            href="/orgs"
            className="text-[0.8125rem] text-kumo-inactive hover:text-kumo-default"
          >
            ← organizations
          </Link>
        }
      />

      <div className="mt-10 grid gap-10 lg:grid-cols-[minmax(0,1fr)_280px]">
        <section>
          <SectionHeader title="repositories" meta={`${repos.length}`} />
          {repos.length === 0 ? (
            <div className="mt-4">
              <EmptyState
                title="no repositories in this org"
                description="create a repository from the dashboard."
              />
            </div>
          ) : (
            <ul className="mt-4 divide-y divide-kumo-hairline border border-kumo-hairline">
              {repos.map((repo) => (
                <li key={repo.name}>
                  <Link
                    href={`/repos/${repo.name}`}
                    className="flex items-center justify-between px-4 py-3.5 hover:bg-kumo-elevated"
                  >
                    <span className="text-[0.9375rem]">{repo.name}</span>
                    <Badge variant="outline">{repo.visibility}</Badge>
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </section>

        <aside>
          <SectionHeader title="members" meta={`${detail.members.length}`} />
          <ul className="mt-4 space-y-2 border border-kumo-hairline p-4">
            {detail.members.map((m) => (
              <li key={m.user_id} className="text-[0.875rem]">
                <span>{m.user_display_name || m.user_name}</span>
                <span className="ml-2 text-kumo-inactive">{m.role}</span>
              </li>
            ))}
          </ul>
          <p className="mt-4 text-[0.8125rem] text-kumo-inactive">
            shared secrets live in{" "}
            <Link
              href="/settings/secrets"
              className="underline hover:text-kumo-default"
            >
              settings → secrets
            </Link>
            .
          </p>
        </aside>
      </div>
    </PageFrame>
  );
}
