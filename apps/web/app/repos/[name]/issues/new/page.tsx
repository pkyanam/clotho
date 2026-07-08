import { Button } from "@cloudflare/kumo";

import { RepoNav } from "src/components/repo-nav";
import { createIssue } from "../actions";

export const dynamic = "force-dynamic";

export default async function NewIssuePage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const action = createIssue.bind(null, name);

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <RepoNav name={name} active="issues" />

      <div className="mt-6 max-w-3xl">
        <h1 className="text-2xl leading-tight">new issue</h1>
        <form action={action} className="mt-6 space-y-4">
          <label className="block text-xs text-kumo-subtle">
            title
            <input
              name="title"
              required
              className="mt-2 block w-full border border-kumo-hairline bg-kumo-base px-3 py-2 text-sm text-kumo-default outline-none focus:border-kumo-contrast"
            />
          </label>
          <label className="block text-xs text-kumo-subtle">
            description
            <textarea
              name="body"
              rows={10}
              className="mt-2 block w-full resize-y border border-kumo-hairline bg-kumo-base px-3 py-2 text-sm text-kumo-default outline-none focus:border-kumo-contrast"
            />
          </label>
          <Button type="submit">create issue</Button>
        </form>
      </div>
    </div>
  );
}
