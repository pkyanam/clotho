"use client";

import { Badge, Breadcrumbs, Button, LayerCard } from "@cloudflare/kumo";
import { PageHeader } from "src/components/kumo/page-header/page-header";
import {
  GitBranchIcon,
  RobotIcon,
  ArrowRightIcon,
  HouseIcon,
} from "@phosphor-icons/react";

const sections = [
  {
    label: "vcs",
    title: "repo browser",
    body: "jj-native repos, commits, and the operation log — browsable.",
  },
  {
    label: "review",
    title: "pr view",
    body: "structured diffs that humans and agents can both review.",
  },
  {
    label: "presence",
    title: "agent sessions",
    body: "live agent checkpoints and session presence on the same repo.",
  },
];

export default function Home() {
  return (
    <div className="mx-auto max-w-6xl px-6 py-16">
      <PageHeader
        breadcrumbs={
          <Breadcrumbs size="sm">
            <Breadcrumbs.Link icon={<HouseIcon size={16} />} href="#">
              clotho
            </Breadcrumbs.Link>
            <Breadcrumbs.Separator />
            <Breadcrumbs.Current>app shell</Breadcrumbs.Current>
          </Breadcrumbs>
        }
        tabs={[
          { label: "overview", value: "overview" },
          { label: "repos", value: "repos" },
          { label: "pull requests", value: "prs" },
          { label: "agents", value: "agents" },
        ]}
        defaultTab="overview"
      >
        <Badge variant="outline">stage 0 · scaffold</Badge>
      </PageHeader>

      <div className="mt-10 flex flex-wrap items-center gap-4">
        <h1
          className="text-balance leading-[1.2]"
          style={{ fontSize: "clamp(1.5rem, 3vw, 1.875rem)" }}
        >
          app shell
        </h1>
        <Button variant="primary" icon={GitBranchIcon}>
          browse repos
        </Button>
        <Button variant="ghost" icon={ArrowRightIcon}>
          read the prd
        </Button>
      </div>

      <p className="mt-4 max-w-xl text-sm text-kumo-subtle">
        repo browser, pr view, and agent-session presence land in stage 6
        (docs/prd.md §5). this shell exists so the design system and workspace
        wiring are real from day zero — now built on cloudflare kumo.
      </p>

      <div className="rule-hairline mt-16" />

      <div className="mt-16 grid gap-4 sm:grid-cols-3">
        {sections.map((s) => (
          <LayerCard key={s.title} className="edge-hover">
            <LayerCard.Secondary>
              <span className="flex items-center gap-2">
                <RobotIcon size={14} />
                {s.label}
              </span>
            </LayerCard.Secondary>
            <LayerCard.Primary>
              <h2 className="text-base">{s.title}</h2>
              <p className="mt-2 text-sm text-kumo-subtle">{s.body}</p>
            </LayerCard.Primary>
          </LayerCard>
        ))}
      </div>
    </div>
  );
}
