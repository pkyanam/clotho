import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import {
  ClothoApiError,
  type Comment,
  type CommitStatus,
  type MergePolicy,
  type Review,
} from "@clotho/sdk-js";

import { api, shortId, timeAgo } from "src/lib/api";
import { DiffView } from "src/components/diff-view";
import { PresencePanel } from "src/components/presence-panel";
import { RepoNav } from "src/components/repo-nav";
import { PageFrame, Panel } from "src/components/ui/page-frame";
import { ThreadEntry } from "src/components/ui/thread-entry";
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

  const client = await api();
  const pull = await client.pull(name, number).catch((e) => {
    if (e instanceof ClothoApiError && e.status === 404) notFound();
    throw e;
  });
  const [diff, statuses, comments, reviews, mergePolicy] = await Promise.all([
    client.pullDiff(name, number),
    client.commitStatuses(name, pull.head.sha).catch(() => [] as CommitStatus[]),
    client.listPullComments(name, number).catch(() => [] as Comment[]),
    client.listPullReviews(name, number).catch(() => [] as Review[]),
    client.getMergePolicy(name).catch(() => defaultMergePolicy()),
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

  const conflictedFiles = diff.files.filter((f) => f.conflicted).length;
  const mergeBlockers = computeMergeBlockers(
    mergePolicy,
    pull.merged,
    pull.state,
    pull.mergeable,
    statuses,
    reviews,
  );
  const canMerge =
    !pull.merged && pull.state === "open" && mergeBlockers.length === 0;
  const threaded = hasThreadedComments(comments);

  return (
    <PageFrame>
      <RepoNav name={name} active="pulls" />

      <div className="mt-6 border-b border-kumo-hairline pb-6">
        <div className="text-[0.8125rem] text-kumo-inactive">
          <Link
            href={`/repos/${name}/pulls`}
            className="hover:text-kumo-default"
          >
            pull requests
          </Link>{" "}
          / #{pull.number}
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-3">
          <h1
            className="min-w-0 text-balance leading-tight text-kumo-default"
            style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
          >
            {pull.title}
          </h1>
          <Badge variant="outline">{pull.merged ? "merged" : pull.state}</Badge>
          {diff.conflicted && <Badge variant="outline">conflicts</Badge>}
        </div>
        <p className="mt-2 text-[0.875rem] text-kumo-inactive">
          {pull.user.login} wants to merge{" "}
          <code className="text-kumo-default">{pull.head.ref}</code> into{" "}
          <code className="text-kumo-default">{pull.base.ref}</code> ·{" "}
          {shortId(diff.from_commit_id)}…{shortId(diff.to_commit_id)} · updated{" "}
          {timeAgo(Date.parse(pull.updated_at))}
        </p>
        {pull.body && (
          <p className="mt-4 max-w-3xl whitespace-pre-wrap text-[0.9375rem] leading-relaxed text-kumo-inactive">
            {pull.body}
          </p>
        )}
      </div>

      {diff.conflicted && (
        <div className="mt-6 max-w-3xl border border-kumo-hairline bg-kumo-base px-4 py-3 text-[0.875rem] leading-relaxed text-kumo-inactive">
          <span className="text-kumo-default">
            {conflictedFiles} {conflictedFiles === 1 ? "file carries" : "files carry"}{" "}
            unresolved conflicts.
          </span>{" "}
          nothing is blocked — clotho records conflicts as first-class objects
          and marks them below. resolve them with a follow-up commit, not a
          queue stall.
        </div>
      )}

      <div className="mt-8 grid gap-8 lg:grid-cols-[220px_minmax(0,1fr)_320px]">
        <aside className="min-w-0">
          <div className="lg:sticky lg:top-20">
            <h2 className="text-[0.9375rem] font-medium text-kumo-default">
              changed files
              <span className="ml-2 text-[0.8125rem] font-normal text-kumo-inactive">
                {diff.files.length}
              </span>
            </h2>
            {diff.files.length === 0 ? (
              <p className="mt-3 text-[0.8125rem] text-kumo-inactive">
                no file changes.
              </p>
            ) : (
              <ul className="mt-3 max-h-[60vh] space-y-1.5 overflow-y-auto">
                {diff.files.map((file) => (
                  <li
                    key={file.path}
                    className="flex items-baseline gap-2 text-[0.8125rem]"
                  >
                    <span className="w-4 shrink-0 text-center text-kumo-inactive">
                      {statusGlyph(file.status)}
                    </span>
                    <span
                      className="min-w-0 truncate text-kumo-default"
                      title={file.path}
                    >
                      {file.path}
                    </span>
                    {file.conflicted && (
                      <span className="shrink-0 text-kumo-inactive">!</span>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </div>
        </aside>

        <div className="min-w-0">
          <h2 className="text-[0.9375rem] font-medium text-kumo-default">
            structured diff
            <span className="ml-2 text-[0.8125rem] font-normal text-kumo-inactive">
              symbol-level changes above line hunks
            </span>
          </h2>
          <DiffView files={diff.files} />
        </div>

        <div className="w-full space-y-5">
          <Panel className="p-4">
            <h2 className="text-[0.9375rem] font-medium text-kumo-default">
              merge
            </h2>
            <p className="mt-2 text-[0.8125rem] leading-relaxed text-kumo-inactive">
              {pull.merged
                ? "this pull request has been merged."
                : pull.state !== "open"
                  ? "this pull request is closed."
                  : mergeBlockers.length === 0
                    ? "ready to merge under the repository policy."
                    : "merge is blocked by repository policy."}
            </p>
            {mergeBlockers.length > 0 && !pull.merged && pull.state === "open" && (
              <ul className="mt-3 space-y-1.5 text-[0.8125rem] text-kumo-inactive">
                {mergeBlockers.map((reason) => (
                  <li key={reason}>· {reason}</li>
                ))}
              </ul>
            )}
            <form action={mergeAction} className="mt-4">
              <Button type="submit" disabled={!canMerge}>
                merge pull request
              </Button>
            </form>
            <p className="mt-3 text-[0.75rem] text-kumo-inactive">
              <Link
                href={`/repos/${name}/settings#merge`}
                className="underline hover:text-kumo-default"
              >
                edit merge policy
              </Link>
            </p>
          </Panel>

          <Panel className="p-4">
            <h2 className="text-[0.9375rem] font-medium text-kumo-default">
              checks
              <span className="ml-2 text-[0.8125rem] font-normal text-kumo-inactive">
                head {shortId(pull.head.sha)}
              </span>
            </h2>
            {statuses.length === 0 ? (
              <p className="mt-3 text-[0.8125rem] text-kumo-inactive">
                no checks reported on this head yet.{" "}
                <Link
                  href={`/repos/${name}/actions`}
                  className="underline hover:text-kumo-default"
                >
                  open actions
                </Link>
              </p>
            ) : (
              <ul className="mt-3 space-y-3">
                {statuses.map((status) => (
                  <li key={status.id} className="text-[0.8125rem]">
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-kumo-default">
                        {status.context || "status"}
                      </span>
                      <Badge variant="outline">{status.state}</Badge>
                    </div>
                    {status.description && (
                      <p className="mt-1 text-kumo-inactive">
                        {status.description}
                      </p>
                    )}
                    {status.target_url && (
                      <Link
                        href={status.target_url}
                        className="mt-1 inline-block text-kumo-inactive underline underline-offset-4 hover:text-kumo-default"
                      >
                        view run
                      </Link>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </Panel>

          <Panel className="p-4">
            <h2 className="text-[0.9375rem] font-medium text-kumo-default">
              review
              <span className="ml-2 text-[0.8125rem] font-normal text-kumo-inactive">
                {countApprovals(reviews)} approved
              </span>
            </h2>
            <form action={approveAction} className="mt-3">
              <textarea
                name="body"
                rows={2}
                placeholder="optional note"
                aria-label="approval note"
                className="block w-full resize-y border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default outline-none placeholder:text-kumo-placeholder focus:border-kumo-contrast"
              />
              <div className="mt-2">
                <Button type="submit">approve</Button>
              </div>
            </form>
            <form
              action={requestChangesAction}
              className="mt-4 border-t border-kumo-hairline pt-4"
            >
              <textarea
                name="body"
                rows={2}
                placeholder="what needs to change?"
                aria-label="requested changes"
                className="block w-full resize-y border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default outline-none placeholder:text-kumo-placeholder focus:border-kumo-contrast"
              />
              <div className="mt-2">
                <Button type="submit" variant="secondary">
                  request changes
                </Button>
              </div>
            </form>
          </Panel>

          <PresencePanel repo={name} />
        </div>
      </div>

      <div className="mt-12 max-w-3xl">
        <h2 className="text-[0.9375rem] font-medium text-kumo-default">
          conversation
          <span className="ml-2 text-[0.8125rem] font-normal text-kumo-inactive">
            {comments.length}
          </span>
        </h2>
        {!threaded && comments.length > 0 && (
          <p className="mt-3 text-[0.8125rem] leading-relaxed text-kumo-inactive">
            inline review threads are limited — comments appear flat. use the
            comment box for discussion; threaded replies appear when the
            collaboration provider returns <code>in_reply_to</code> metadata.
          </p>
        )}
        <div className="mt-4 space-y-4">
          {comments.length ? (
            threaded ? (
              <CommentThreads comments={comments} />
            ) : (
              comments.map((comment) => (
                <ThreadEntry
                  key={comment.id}
                  author={comment.user.login}
                  meta={timeAgo(Date.parse(comment.updated_at))}
                  body={comment.body}
                />
              ))
            )
          ) : (
            <p className="border border-kumo-hairline bg-kumo-base px-4 py-6 text-[0.875rem] text-kumo-inactive">
              no comments yet. reviews and discussion land here.
            </p>
          )}

          <form
            action={commentAction}
            className="border border-kumo-hairline bg-kumo-base"
          >
            <div className="border-b border-kumo-hairline px-4 py-3 text-[0.875rem] text-kumo-default">
              add a comment
            </div>
            <div className="p-4">
              <textarea
                name="body"
                required
                rows={4}
                placeholder="write in plain language — agents read this thread too."
                aria-label="comment body"
                className="block w-full resize-y border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.9375rem] text-kumo-default outline-none placeholder:text-kumo-placeholder focus:border-kumo-contrast"
              />
              <div className="mt-3">
                <Button type="submit">comment</Button>
              </div>
            </div>
          </form>
        </div>
      </div>
    </PageFrame>
  );
}

function CommentThreads({ comments }: { comments: Comment[] }) {
  const roots = comments.filter(
    (c) => c.in_reply_to == null || c.in_reply_to === 0,
  );
  const repliesByParent = new Map<number, Comment[]>();
  for (const comment of comments) {
    const parent = comment.in_reply_to;
    if (parent == null || parent === 0) continue;
    const list = repliesByParent.get(parent) ?? [];
    list.push(comment);
    repliesByParent.set(parent, list);
  }

  return (
    <>
      {roots.map((root) => (
        <div key={root.id} className="space-y-2">
          <ThreadEntry
            author={root.user.login}
            meta={timeAgo(Date.parse(root.updated_at))}
            body={root.body}
          />
          {(repliesByParent.get(root.id) ?? []).map((reply) => (
            <div key={reply.id} className="ml-6 border-l border-kumo-hairline pl-4">
              <ThreadEntry
                author={reply.user.login}
                meta={timeAgo(Date.parse(reply.updated_at))}
                body={reply.body}
              />
            </div>
          ))}
        </div>
      ))}
    </>
  );
}

function defaultMergePolicy(): MergePolicy {
  return {
    require_passing_actions: false,
    block_merge_when_conflicted: true,
    require_review_approvals: 0,
    protect_default_branch: false,
    updated_at: "",
  };
}

function hasThreadedComments(comments: Comment[]): boolean {
  return comments.some(
    (c) => c.in_reply_to != null && c.in_reply_to > 0,
  );
}

function countApprovals(reviews: Review[]): number {
  const seen = new Set<string>();
  let count = 0;
  for (const review of reviews) {
    if (seen.has(review.user.login)) continue;
    seen.add(review.user.login);
    if (review.state.toUpperCase() === "APPROVED") count += 1;
  }
  return count;
}

function computeMergeBlockers(
  policy: MergePolicy,
  merged: boolean,
  state: string,
  mergeable: boolean,
  statuses: CommitStatus[],
  reviews: Review[],
): string[] {
  if (merged || state !== "open") return [];

  const blockers: string[] = [];
  if (policy.block_merge_when_conflicted && !mergeable) {
    blockers.push(
      "pull request has merge conflicts with the base branch",
    );
  }
  if (policy.require_passing_actions) {
    if (statuses.length === 0) {
      blockers.push("actions have not reported status on the head commit");
    } else {
      for (const status of statuses) {
        const s = status.state.toLowerCase();
        const ctx = status.context || "check";
        if (s === "failure" || s === "error") {
          blockers.push(`${ctx} reported ${s}`);
        } else if (s === "pending") {
          blockers.push(`${ctx} is still pending`);
        }
      }
    }
  }
  if (policy.require_review_approvals > 0) {
    const approvals = countApprovals(reviews);
    if (approvals < policy.require_review_approvals) {
      blockers.push(
        `requires ${policy.require_review_approvals} approving review(s), found ${approvals}`,
      );
    }
  }
  return blockers;
}

function statusGlyph(status: string): string {
  switch (status) {
    case "added":
      return "+";
    case "removed":
    case "deleted":
      return "−";
    default:
      return "~";
  }
}
