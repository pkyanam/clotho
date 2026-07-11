import Link from "next/link";

import {
  PageFrame,
  PageTitle,
  Panel,
  SectionHeader,
} from "src/components/ui/page-frame";
import { SettingsNav } from "src/components/settings-nav";

export default function SettingsHubPage() {
  return (
    <PageFrame>
      <PageTitle
        title="settings"
        description="account, organization, compute, and secrets — the platform console."
      />
      <div className="mt-6">
        <SettingsNav active="overview" />
      </div>

      <div className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <SettingsCard
          href="/settings/appearance"
          title="appearance"
          body="color scheme — system, dark, or light. dark is the default clotho look."
        />
        <SettingsCard
          href="/settings/compute"
          title="compute"
          body="providers, defaults, and connection status for Actions and sandboxes."
        />
        <SettingsCard
          href="/settings/secrets"
          title="secrets"
          body="org-scoped credentials. never returned after save — rotate any time."
        />
        <SettingsCard
          href="/orgs"
          title="organizations"
          body="membership and ownership for repositories and shared secrets."
        />
      </div>
    </PageFrame>
  );
}

function SettingsCard({
  href,
  title,
  body,
}: {
  href: string;
  title: string;
  body: string;
}) {
  return (
    <Link href={href} className="block">
      <Panel className="h-full p-5 transition-colors hover:bg-kumo-elevated">
        <SectionHeader title={title} />
        <p className="mt-3 text-[0.875rem] leading-relaxed text-kumo-inactive">
          {body}
        </p>
      </Panel>
    </Link>
  );
}
