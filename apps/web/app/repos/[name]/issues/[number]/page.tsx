import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, timeAgo } from "src/lib/api";
import { IssueMetadataFields } from "src/components/issue-metadata-fields";
import { RepoNav } from "src/components/repo-nav";
import { MetaRow, PageFrame, Panel } from "src/components/ui/page-frame";
import { ThreadEntry } from "src/components/ui/thread-entry";
import { commentOnIssue, updateIssueMetadata } from "../actions";

export const dynamic = "force-dynamic";

export default async function IssuePage({
  params,
}: {
  params: Promise<{ name: string; number: string }>;
}) {
  const { name, number: rawNumber } = await params;
  const number = Number(rawNumber);
  if (!Number.isInteger(number) || number < 1) notFound();

  const client = await api();
  const detail = await client.issue(name, number).catch((e) => {
    if (e instanceof ClothoApiError && e.status === 404) notFound();
    throw e;
  });
  const [labels, repo] = await Promise.all([
    client.listLabels(name).catch(() => []),
    client.getRepo(name).catch(() => null),
  ]);
  const ownerOrg = repo?.owner_org || repo?.owner;
  const org = ownerOrg
    ? await client.getOrg(ownerOrg).catch(() => null)
    : null;
  const members = org?.members ?? [];

  const commentAction = commentOnIssue.bind(null, name, number);
  const metadataAction = updateIssueMetadata.bind(null, name, number);
  const { issue, comments } = detail;

  return (
    <PageFrame>
      <RepoNav name={name} active="issues" />

      <div className="mt-6 border-b border-kumo-hairline pb-6">
        <div className="text-[0.8125rem] text-kumo-inactive">
          <Link
            href={`/repos/${name}/issues`}
            className="hover:text-kumo-default"
          >
            issues
          </Link>{" "}
          / #{issue.number}
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-3">
          <h1
            className="min-w-0 text-balance leading-tight text-kumo-default"
            style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
          >
            {issue.title}
          </h1>
          <Badge variant="outline">{issue.state}</Badge>
          {issue.labels.map((label) => (
            <Badge key={label.name} variant="outline">
              {label.name}
            </Badge>
          ))}
        </div>
        <p className="mt-2 text-[0.875rem] text-kumo-inactive">
          opened by {issue.user.login}
          {issue.assignees.length > 0
            ? ` · assigned to ${issue.assignees.map((a) => a.login).join(", ")}`
            : ""}
          {issue.milestone ? ` · ${issue.milestone.title}` : ""} · updated{" "}
          {timeAgo(Date.parse(issue.updated_at))}
        </p>
      </div>

      <div className="mt-8 grid gap-8 lg:grid-cols-[minmax(0,1fr)_300px]">
        <div className="min-w-0 space-y-4">
          <ThreadEntry
            author={issue.user.login}
            meta={`opened ${timeAgo(Date.parse(issue.created_at))}`}
            body={issue.body || "no description provided."}
            muted={!issue.body}
          />

          {comments.map((comment) => (
            <ThreadEntry
              key={comment.id}
              author={comment.user.login}
              meta={timeAgo(Date.parse(comment.updated_at))}
              body={comment.body}
            />
          ))}

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
                rows={5}
                placeholder="write in plain language — agents read this thread too. @name mentions are best-effort."
                aria-label="comment body"
                className="block w-full resize-y border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.9375rem] text-kumo-default outline-none placeholder:text-kumo-placeholder focus:border-kumo-contrast"
              />
              <div className="mt-3">
                <Button type="submit">comment</Button>
              </div>
            </div>
          </form>
        </div>

        <aside className="space-y-5">
          <Panel className="p-4">
            <h2 className="text-[0.9375rem] font-medium text-kumo-default">
              details
            </h2>
            <dl className="mt-2">
              <MetaRow label="state" value={issue.state} />
              <MetaRow label="author" value={issue.user.login} />
              <MetaRow
                label="assignees"
                value={
                  issue.assignees.length > 0
                    ? issue.assignees.map((a) => a.login).join(", ")
                    : "none"
                }
              />
              <MetaRow
                label="milestone"
                value={issue.milestone?.title ?? "none"}
              />
              <MetaRow
                label="opened"
                value={timeAgo(Date.parse(issue.created_at))}
              />
              <MetaRow label="comments" value={String(comments.length)} />
            </dl>
          </Panel>

          {(labels.length > 0 || members.length > 0) && (
            <Panel className="p-4">
              <h2 className="text-[0.9375rem] font-medium text-kumo-default">
                labels & assignee
              </h2>
              <form action={metadataAction} className="mt-3 space-y-4">
                <IssueMetadataFields
                  labels={labels}
                  members={members}
                  defaultLabels={issue.labels.map((l) => l.name)}
                  defaultAssignees={issue.assignees.map((a) => a.login)}
                />
                <Button type="submit" variant="secondary">
                  save
                </Button>
              </form>
            </Panel>
          )}
        </aside>
      </div>
    </PageFrame>
  );
}
