import { Button } from "@cloudflare/kumo";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ClothoApiError } from "@clotho/sdk-js";

import { api } from "src/lib/api";
import { IssueMetadataFields } from "src/components/issue-metadata-fields";
import { RepoNav } from "src/components/repo-nav";
import { PageFrame } from "src/components/ui/page-frame";
import { createIssue } from "../actions";

export const dynamic = "force-dynamic";

export default async function NewIssuePage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const client = await api();
  const [labels, repo] = await Promise.all([
    client.listLabels(name).catch(() => []),
    client.getRepo(name).catch((e) => {
      if (e instanceof ClothoApiError && e.status === 404) notFound();
      throw e;
    }),
  ]);
  const ownerOrg = repo.owner_org || repo.owner;
  const org = await client.getOrg(ownerOrg).catch(() => null);
  const members = org?.members ?? [];
  const action = createIssue.bind(null, name);

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
          / new
        </div>
        <h1
          className="mt-2 leading-tight text-kumo-default"
          style={{ fontSize: "clamp(1.375rem, 2.5vw, 1.75rem)" }}
        >
          new issue
        </h1>
        <p className="mt-2 max-w-3xl text-[0.9375rem] leading-relaxed text-kumo-inactive">
          issues are a shared queue for humans and agents. write clearly —
          agents pick these up as work.
        </p>
      </div>

      <form action={action} className="mt-8 max-w-3xl space-y-5">
        <label className="block text-[0.8125rem] text-kumo-inactive">
          title
          <input
            name="title"
            required
            placeholder="a short, specific summary"
            className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-base px-3 py-2.5 text-[0.9375rem] text-kumo-default outline-none placeholder:text-kumo-placeholder focus:border-kumo-contrast"
          />
        </label>
        <label className="block text-[0.8125rem] text-kumo-inactive">
          description
          <textarea
            name="body"
            rows={10}
            placeholder="context, expected behavior, and anything an agent needs to act on this."
            className="mt-1.5 block w-full resize-y border border-kumo-hairline bg-kumo-base px-3 py-2.5 text-[0.9375rem] text-kumo-default outline-none placeholder:text-kumo-placeholder focus:border-kumo-contrast"
          />
        </label>
        <IssueMetadataFields labels={labels} members={members} />
        <div className="flex items-center gap-3">
          <Button type="submit">create issue</Button>
          <Link
            href={`/repos/${name}/issues`}
            className="text-[0.875rem] text-kumo-inactive hover:text-kumo-default"
          >
            cancel
          </Link>
        </div>
      </form>
    </PageFrame>
  );
}
