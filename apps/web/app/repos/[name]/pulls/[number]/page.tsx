import { Badge } from "@cloudflare/kumo";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, shortId, timeAgo } from "src/lib/api";
import { DiffView } from "src/components/diff-view";
import { PresencePanel } from "src/components/presence-panel";
import { RepoNav } from "src/components/repo-nav";

export const dynamic = "force-dynamic";

export default async function PullPage({
  params,
}: {
  params: Promise<{ name: string; number: string }>;
}) {
  const { name, number: rawNumber } = await params;
  const number = Number(rawNumber);
  if (!Number.isInteger(number) || number < 1) notFound();

  const client = api();
  const pull = await client.pull(name, number).catch((e) => {
    if (e instanceof ClothoApiError && e.status === 404) notFound();
    throw e;
  });
  const diff = await client.pullDiff(name, number);

  return (
    <div className="mx-auto max-w-6xl px-6 py-16">
      <RepoNav name={name} active="pulls" />

      <div className="mt-8 flex flex-wrap items-center gap-3">
        <h1
          className="leading-[1.2]"
          style={{ fontSize: "clamp(1.25rem, 2.5vw, 1.5rem)" }}
        >
          {pull.title}
        </h1>
        <Badge variant="outline">{pull.merged ? "merged" : pull.state}</Badge>
        {diff.conflicted && <Badge variant="outline">conflict</Badge>}
      </div>
      <p className="mt-2 text-xs text-kumo-inactive">
        #{pull.number} · {pull.user.login} · {pull.head.ref} → {pull.base.ref} ·{" "}
        {shortId(diff.from_commit_id)}…{shortId(diff.to_commit_id)} · updated{" "}
        {timeAgo(Date.parse(pull.updated_at))} ·{" "}
        <a href={pull.html_url} className="hover:text-kumo-default">
          in forgejo ↗
        </a>
      </p>
      {pull.body && (
        <p className="mt-4 max-w-2xl whitespace-pre-wrap text-sm text-kumo-subtle">
          {pull.body}
        </p>
      )}

      {diff.conflicted && (
        <p className="mt-6 max-w-2xl border border-kumo-hairline px-4 py-3 text-xs text-kumo-subtle">
          this change carries unresolved jj conflicts. nothing is blocked — the
          conflicted files landed as first-class objects and are marked below;
          resolution is a follow-up commit, not a queue stall.
        </p>
      )}

      <div className="mt-8 flex flex-col gap-8 lg:flex-row">
        <div className="min-w-0 grow">
          <h2 className="text-xs text-kumo-subtle">
            structured diff · {diff.files.length}{" "}
            {diff.files.length === 1 ? "file" : "files"}
          </h2>
          <DiffView files={diff.files} />
        </div>
        <div className="w-full shrink-0 lg:w-[340px]">
          <PresencePanel repo={name} />
        </div>
      </div>
    </div>
  );
}
