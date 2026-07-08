import { Badge, Button } from "@cloudflare/kumo";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, shortId, timeAgo } from "src/lib/api";
import { DiffView } from "src/components/diff-view";
import { PresencePanel } from "src/components/presence-panel";
import { RepoNav } from "src/components/repo-nav";
import { commentOnPull, mergePull, reviewPull } from "../actions";

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
  const [diff, statuses, issueThread] = await Promise.all([
    client.pullDiff(name, number),
    client.commitStatuses(name, pull.head.sha).catch(() => []),
    client.issue(name, number).catch(() => null),
  ]);
  const commentAction = commentOnPull.bind(null, name, number);
  const approveAction = reviewPull.bind(null, name, number, "APPROVE");
  const requestChangesAction = reviewPull.bind(
    null,
    name,
    number,
    "REQUEST_CHANGES",
  );
  const mergeAction = mergePull.bind(null, name, number);

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="pulls" />

      <div className="mt-6 flex flex-wrap items-center gap-3">
        <h1 className="min-w-0 text-2xl leading-tight">{pull.title}</h1>
        <Badge variant="outline">{pull.merged ? "merged" : pull.state}</Badge>
        {diff.conflicted && <Badge variant="outline">conflict</Badge>}
      </div>
      <p className="mt-2 text-xs text-kumo-inactive">
        #{pull.number} · {pull.user.login} · {pull.head.ref} → {pull.base.ref} ·{" "}
        {shortId(diff.from_commit_id)}…{shortId(diff.to_commit_id)} · updated{" "}
        {timeAgo(Date.parse(pull.updated_at))}
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

      <div className="mt-8 grid gap-8 lg:grid-cols-[220px_minmax(0,1fr)_340px]">
        <aside className="space-y-4">
          <section className="border border-kumo-hairline p-3">
            <h2 className="text-xs text-kumo-subtle">changed files</h2>
            <ul className="mt-3 space-y-2">
              {diff.files.map((file) => (
                <li key={file.path} className="truncate text-xs">
                  <span className="text-kumo-inactive">{file.status}</span>{" "}
                  {file.path}
                </li>
              ))}
            </ul>
          </section>
        </aside>

        <div className="min-w-0">
          <h2 className="text-sm">
            structured diff · {diff.files.length}{" "}
            {diff.files.length === 1 ? "file" : "files"}
          </h2>
          <DiffView files={diff.files} />
        </div>

        <div className="w-full space-y-6">
          <section className="border border-kumo-hairline p-4">
            <h2 className="text-sm">merge</h2>
            <div className="mt-3 space-y-2 text-xs text-kumo-inactive">
              <p>{pull.mergeable ? "mergeable" : "not mergeable"}</p>
              <p>{statuses.length} commit statuses on head {shortId(pull.head.sha)}</p>
            </div>
            <form action={mergeAction} className="mt-4">
              <Button
                type="submit"
                disabled={pull.merged || pull.state !== "open" || !pull.mergeable}
              >
                merge pull request
              </Button>
            </form>
          </section>

          <section className="border border-kumo-hairline p-4">
            <h2 className="text-sm">checks</h2>
            {statuses.length === 0 ? (
              <p className="mt-3 text-xs text-kumo-inactive">no statuses yet.</p>
            ) : (
              <ul className="mt-3 space-y-2">
                {statuses.map((status) => (
                  <li key={status.id} className="text-xs">
                    <div className="flex items-center justify-between gap-2">
                      <span>{status.context || "status"}</span>
                      <Badge variant="outline">{status.state}</Badge>
                    </div>
                    {status.description && (
                      <p className="mt-1 text-kumo-inactive">
                        {status.description}
                      </p>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </section>

          <PresencePanel repo={name} />
        </div>
      </div>

      <div className="mt-10 grid gap-6 lg:grid-cols-[minmax(0,1fr)_340px]">
        <section className="space-y-4">
          <h2 className="text-sm">conversation</h2>
          {issueThread?.comments.length ? (
            issueThread.comments.map((comment) => (
              <article key={comment.id} className="border border-kumo-hairline p-4">
                <div className="mb-3 text-xs text-kumo-inactive">
                  {comment.user.login} · {timeAgo(Date.parse(comment.updated_at))}
                </div>
                <p className="whitespace-pre-wrap text-sm text-kumo-subtle">
                  {comment.body}
                </p>
              </article>
            ))
          ) : (
            <p className="border border-kumo-hairline px-4 py-6 text-sm text-kumo-inactive">
              no comments yet.
            </p>
          )}
        </section>

        <section className="space-y-4">
          <form action={commentAction} className="border border-kumo-hairline p-4">
            <label className="block text-xs text-kumo-subtle">
              comment
              <textarea
                name="body"
                required
                rows={5}
                className="mt-2 block w-full resize-y border border-kumo-hairline bg-kumo-base px-3 py-2 text-sm text-kumo-default outline-none focus:border-kumo-contrast"
              />
            </label>
            <div className="mt-3">
              <Button type="submit">comment</Button>
            </div>
          </form>

          <form action={approveAction} className="border border-kumo-hairline p-4">
            <label className="block text-xs text-kumo-subtle">
              approve with note
              <textarea
                name="body"
                rows={3}
                className="mt-2 block w-full resize-y border border-kumo-hairline bg-kumo-base px-3 py-2 text-sm text-kumo-default outline-none focus:border-kumo-contrast"
              />
            </label>
            <div className="mt-3">
              <Button type="submit">approve</Button>
            </div>
          </form>

          <form
            action={requestChangesAction}
            className="border border-kumo-hairline p-4"
          >
            <label className="block text-xs text-kumo-subtle">
              request changes
              <textarea
                name="body"
                rows={3}
                className="mt-2 block w-full resize-y border border-kumo-hairline bg-kumo-base px-3 py-2 text-sm text-kumo-default outline-none focus:border-kumo-contrast"
              />
            </label>
            <div className="mt-3">
              <Button type="submit" variant="secondary">
                request changes
              </Button>
            </div>
          </form>
        </section>
      </div>
    </div>
  );
}
