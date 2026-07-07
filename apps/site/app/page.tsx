"use client";

import { Badge, Button, LayerCard } from "@cloudflare/kumo";
import {
  GitBranchIcon,
  StackIcon,
  RobotIcon,
  ArrowRightIcon,
} from "@phosphor-icons/react";

const pillars = [
  {
    icon: GitBranchIcon,
    title: "jj-native engine",
    body: "built on jujutsu, git-compatible to the byte. operation log as an api, conflicts that never block, no staging area to forget.",
  },
  {
    icon: StackIcon,
    title: "arachne storage",
    body: "xet-style chunk-level dedup. multi-gigabyte models and datasets stored once, moved fast, reconstructed exactly.",
  },
  {
    icon: RobotIcon,
    title: "agent-native interface",
    body: "agents are first-class identities — scoped credentials, checkpoints, structured diffs. not humans with a bot flag.",
  },
];

export default function Home() {
  return (
    <main className="mx-auto flex min-h-dvh max-w-4xl flex-col justify-center px-6 py-24">
      <div className="reveal">
        <Badge variant="outline">prototype in progress</Badge>
      </div>

      <h1
        className="reveal mt-8 text-balance leading-[1.1]"
        style={{
          fontSize: "clamp(2.25rem, 6vw, 3.75rem)",
          ["--reveal-delay" as string]: "80ms",
        }}
      >
        clotho
      </h1>

      <p
        className="reveal mt-6 max-w-2xl text-lg text-kumo-subtle"
        style={{ ["--reveal-delay" as string]: "160ms" }}
      >
        version control for humans and ai agents — working together, on the
        same repo, at the same time.
      </p>

      <div
        className="reveal mt-10 flex flex-wrap gap-3"
        style={{ ["--reveal-delay" as string]: "200ms" }}
      >
        <Button variant="primary" icon={ArrowRightIcon}>
          get started
        </Button>
        <Button variant="ghost">read the docs</Button>
      </div>

      <div
        className="rule-hairline reveal mt-16"
        style={{ ["--reveal-delay" as string]: "240ms" }}
      />

      <div
        className="reveal mt-16 grid gap-4 sm:grid-cols-3"
        style={{ ["--reveal-delay" as string]: "320ms" }}
      >
        {pillars.map((pillar) => (
          <LayerCard key={pillar.title} className="edge-hover">
            <LayerCard.Secondary>
              <span className="flex items-center gap-2">
                <pillar.icon size={14} />
                {pillar.title}
              </span>
            </LayerCard.Secondary>
            <LayerCard.Primary>
              <p className="text-sm text-kumo-subtle">{pillar.body}</p>
            </LayerCard.Primary>
          </LayerCard>
        ))}
      </div>

      <p
        className="reveal mt-16 text-xs text-kumo-subtle"
        style={{ ["--reveal-delay" as string]: "400ms" }}
      >
        open source, apache-2.0. modular by design — compute, storage,
        database, and network are all swappable.
      </p>
    </main>
  );
}
