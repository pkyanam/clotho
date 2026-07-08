import { Button } from "@cloudflare/kumo";

import { createRepo } from "./actions";

export const dynamic = "force-dynamic";

export default async function NewRepoPage() {
  const action = createRepo;

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <div className="border-b border-kumo-hairline pb-6">
        <h1 className="text-2xl leading-tight">new repository</h1>
        <p className="mt-2 max-w-2xl text-sm text-kumo-subtle">
          create a jj-native repo owned by Clotho. Forgejo is provisioned behind
          the scenes as the collaboration provider.
        </p>
      </div>

      <form action={action} className="mt-8 max-w-3xl space-y-4">
        <label className="block text-xs text-kumo-subtle">
          name
          <input
            name="name"
            required
            pattern="[a-z0-9-_]+"
            placeholder="weave"
            className="mt-2 block w-full border border-kumo-hairline bg-kumo-base px-3 py-2 text-sm text-kumo-default outline-none focus:border-kumo-contrast"
          />
        </label>

        <label className="block text-xs text-kumo-subtle">
          description
          <input
            name="description"
            className="mt-2 block w-full border border-kumo-hairline bg-kumo-base px-3 py-2 text-sm text-kumo-default outline-none focus:border-kumo-contrast"
          />
        </label>

        <label className="block text-xs text-kumo-subtle">
          visibility
          <select
            name="visibility"
            defaultValue="public"
            className="mt-2 block w-full border border-kumo-hairline bg-kumo-base px-3 py-2 text-sm text-kumo-default outline-none focus:border-kumo-contrast"
          >
            <option value="public">public</option>
            <option value="private">private</option>
            <option value="internal">internal</option>
          </select>
        </label>

        <Button type="submit">create repository</Button>
      </form>
    </div>
  );
}
