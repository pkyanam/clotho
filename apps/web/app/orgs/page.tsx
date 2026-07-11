import Link from "next/link";

import { api } from "src/lib/api";
import {
  EmptyState,
  PageFrame,
  PageTitle,
} from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

export default async function OrgsPage() {
  const orgs = await (await api())
    .orgs()
    .catch(() => []);

  return (
    <PageFrame>
      <PageTitle
        title="organizations"
        description="teams and ownership for repositories and shared secrets."
      />

      {orgs.length === 0 ? (
        <div className="mt-10">
          <EmptyState
            title="no organizations"
            description="the bootstrap organization is created when the control plane starts."
          />
        </div>
      ) : (
        <ul className="mt-8 divide-y divide-kumo-hairline border border-kumo-hairline">
          {orgs.map((org) => (
            <li key={org.name}>
              <Link
                href={`/orgs/${org.name}`}
                className="flex flex-wrap items-center justify-between gap-3 px-4 py-4 transition-colors hover:bg-kumo-elevated"
              >
                <span>
                  <span className="block text-[0.9375rem]">
                    {org.display_name || org.name}
                  </span>
                  <span className="mt-1 block text-[0.8125rem] text-kumo-inactive">
                    {org.name}
                  </span>
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </PageFrame>
  );
}
