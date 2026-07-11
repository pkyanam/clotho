import { Badge, Button } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, timeAgo } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import { FilterTab } from "src/components/ui/filter-tabs";
import { EmptyState, PageFrame } from "src/components/ui/page-frame";

export const dynamic = "force-dynamic";

type StateFilter = "open" | "closed" | "all";

function parseState(raw: string | undefined): StateFilter {
  return raw === "closed" || raw === "all" ? raw : "open";
}

function issueHref(
  name: string,
  state: StateFilter,
  label?: string,
  assignee?: string,
): string {
  const params = new URLSearchParams();
  if (state !== "open") params.set("state", state);
  if (label) params.set("label", label);
  if (assignee) params.set("assignee", assignee);
  const qs = params.toString();
  return `/repos/${name}/issues${qs ? `?${qs}` : ""}`;
}

export default async function IssuesPage({
  params,
  searchParams,
}: {
  params: Promise<{ name: string }>;
  searchParams: Promise<{ state?: string; label?: string; assignee?: string }>;
}) {
  const { name } = await params;
  const sp = await searchParams;
  const state = parseState(sp.state);
  const labelFilter = sp.label?.trim() || undefined;
  const assigneeFilter = sp.assignee?.trim() || undefined;

  const client = await api();
  const [issues, labels, repo] = await Promise.all([
    client
      .issues(name, "all", {
        labels: labelFilter,
        assignee: assigneeFilter,
      })
      .catch((e) => {
        if (e instanceof ClothoApiError && e.status === 404) notFound();
        throw e;
      }),
    client.listLabels(name).catch(() => []),
    client.getRepo(name).catch(() => null),
  ]);

  const ownerOrg = repo?.owner_org || repo?.owner;
  const org = ownerOrg
    ? await client.getOrg(ownerOrg).catch(() => null)
    : null;
  const members = org?.members ?? [];

  const open = issues.filter((issue) => issue.state === "open");
  const closed = issues.filter((issue) => issue.state !== "open");
  const visible =
    state === "open" ? open : state === "closed" ? closed : issues;

  const labelNames = [
    ...new Set([
      ...labels.map((l) => l.name),
      ...issues.flatMap((i) => i.labels.map((l) => l.name)),
    ]),
  ].sort();
  const assigneeNames = [
    ...new Set([
      ...members.map((m) => m.user_name),
      ...issues.flatMap((i) => i.assignees.map((a) => a.login)),
    ]),
  ].sort();

  return (
    <PageFrame>
      <RepoNav name={name} active="issues" />

      <div className="mt-6 flex flex-wrap items-start justify-between gap-4 border-b border-kumo-hairline pb-6">
        <div className="min-w-0 max-w-3xl">
          <h1
            className="leading-tight text-kumo-default"
            style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
          >
            issues
          </h1>
          <p className="mt-2 text-[0.9375rem] leading-relaxed text-kumo-inactive">
            triage work, agent findings, and human follow-ups.
          </p>
        </div>
        <Link href={`/repos/${name}/issues/new`}>
          <Button type="button">new issue</Button>
        </Link>
      </div>

      <div className="mt-6 flex flex-wrap items-center gap-1">
        <FilterTab
          href={issueHref(name, "open", labelFilter, assigneeFilter)}
          label={`open · ${open.length}`}
          current={state === "open"}
        />
        <FilterTab
          href={issueHref(name, "closed", labelFilter, assigneeFilter)}
          label={`closed · ${closed.length}`}
          current={state === "closed"}
        />
        <FilterTab
          href={issueHref(name, "all", labelFilter, assigneeFilter)}
          label={`all · ${issues.length}`}
          current={state === "all"}
        />
      </div>

      {(labelNames.length > 0 || assigneeNames.length > 0) && (
        <div className="mt-4 flex flex-wrap items-center gap-2">
          <span className="text-[0.75rem] text-kumo-inactive">filter:</span>
          {labelFilter ? (
            <Link href={issueHref(name, state, undefined, assigneeFilter)}>
              <Badge variant="outline">label: {labelFilter} ×</Badge>
            </Link>
          ) : (
            labelNames.slice(0, 8).map((label) => (
              <Link key={label} href={issueHref(name, state, label, assigneeFilter)}>
                <Badge variant="outline">{label}</Badge>
              </Link>
            ))
          )}
          {assigneeFilter ? (
            <Link href={issueHref(name, state, labelFilter, undefined)}>
              <Badge variant="outline">assignee: {assigneeFilter} ×</Badge>
            </Link>
          ) : (
            assigneeNames.slice(0, 6).map((login) => (
              <Link
                key={login}
                href={issueHref(name, state, labelFilter, login)}
              >
                <Badge variant="outline">@{login}</Badge>
              </Link>
            ))
          )}
        </div>
      )}

      {visible.length === 0 ? (
        <div className="mt-6">
          <EmptyState
            title={
              state === "open"
                ? "no open issues"
                : state === "closed"
                  ? "no closed issues"
                  : "no issues yet"
            }
            description={
              issues.length === 0
                ? "open the first issue to give humans and agents a shared queue of work on this repository."
                : "nothing matches this filter."
            }
            action={
              issues.length === 0 ? (
                <Link href={`/repos/${name}/issues/new`}>
                  <Button type="button">open the first issue</Button>
                </Link>
              ) : undefined
            }
          />
        </div>
      ) : (
        <ul className="mt-6 divide-y divide-kumo-hairline border border-kumo-hairline">
          {visible.map((issue) => (
            <li key={issue.number}>
              <Link
                href={`/repos/${name}/issues/${issue.number}`}
                className="flex flex-col gap-2 px-4 py-3.5 transition-colors hover:bg-kumo-elevated sm:flex-row sm:items-baseline sm:justify-between"
              >
                <span className="min-w-0">
                  <span className="flex flex-wrap items-center gap-2">
                    <span className="truncate text-[0.9375rem] text-kumo-default">
                      {issue.title}
                    </span>
                    <Badge variant="outline">{issue.state}</Badge>
                    {issue.labels.map((label) => (
                      <Badge key={label.name} variant="outline">
                        {label.name}
                      </Badge>
                    ))}
                  </span>
                  <span className="mt-1 block text-[0.8125rem] text-kumo-inactive">
                    #{issue.number} opened by {issue.user.login}
                    {issue.assignees.length > 0
                      ? ` · assigned to ${issue.assignees.map((a) => a.login).join(", ")}`
                      : ""}
                    {issue.comments > 0
                      ? ` · ${issue.comments} ${issue.comments === 1 ? "comment" : "comments"}`
                      : ""}
                  </span>
                </span>
                <span className="shrink-0 text-[0.8125rem] text-kumo-inactive">
                  {timeAgo(Date.parse(issue.updated_at))}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </PageFrame>
  );
}
