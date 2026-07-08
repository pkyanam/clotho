import { Badge, Button } from "@cloudflare/kumo";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api, timeAgo } from "src/lib/api";
import { RepoNav } from "src/components/repo-nav";
import { commentOnIssue } from "../actions";

export const dynamic = "force-dynamic";

export default async function IssuePage({
  params,
}: {
  params: Promise<{ name: string; number: string }>;
}) {
  const { name, number: rawNumber } = await params;
  const number = Number(rawNumber);
  if (!Number.isInteger(number) || number < 1) notFound();

  const detail = await api()
    .issue(name, number)
    .catch((e) => {
      if (e instanceof ClothoApiError && e.status === 404) notFound();
      throw e;
    });
  const action = commentOnIssue.bind(null, name, number);

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="issues" />

      <div className="mt-6 flex flex-wrap items-center gap-3">
        <h1 className="min-w-0 text-2xl leading-tight">{detail.issue.title}</h1>
        <Badge variant="outline">{detail.issue.state}</Badge>
      </div>
      <p className="mt-2 text-xs text-kumo-inactive">
        #{detail.issue.number} · {detail.issue.user.login} · updated{" "}
        {timeAgo(Date.parse(detail.issue.updated_at))}
      </p>

      <div className="mt-8 grid gap-8 lg:grid-cols-[minmax(0,1fr)_340px]">
        <div className="min-w-0 space-y-6">
          <article className="border border-kumo-hairline p-4">
            <div className="mb-3 text-xs text-kumo-inactive">
              opened {timeAgo(Date.parse(detail.issue.created_at))}
            </div>
            <p className="whitespace-pre-wrap text-sm text-kumo-subtle">
              {detail.issue.body || "no description."}
            </p>
          </article>

          {detail.comments.map((comment) => (
            <article key={comment.id} className="border border-kumo-hairline p-4">
              <div className="mb-3 text-xs text-kumo-inactive">
                {comment.user.login} · {timeAgo(Date.parse(comment.updated_at))}
              </div>
              <p className="whitespace-pre-wrap text-sm text-kumo-subtle">
                {comment.body}
              </p>
            </article>
          ))}

          <form action={action} className="border border-kumo-hairline p-4">
            <label className="block text-xs text-kumo-subtle">
              add comment
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
        </div>

        <aside className="space-y-4">
          <section className="border border-kumo-hairline p-4 text-xs">
            <h2 className="text-sm">labels</h2>
            <div className="mt-3 flex flex-wrap gap-2">
              {detail.issue.labels.length === 0 ? (
                <span className="text-kumo-inactive">none</span>
              ) : (
                detail.issue.labels.map((label) => (
                  <Badge key={label.name} variant="outline">
                    {label.name}
                  </Badge>
                ))
              )}
            </div>
          </section>
        </aside>
      </div>
    </div>
  );
}
