import { Button } from "@cloudflare/kumo";
import Link from "next/link";

import {
  PageFrame,
  PageTitle,
} from "src/components/ui/page-frame";
import { createRepo } from "./actions";

export const dynamic = "force-dynamic";

export default async function NewRepoPage() {
  return (
    <PageFrame>
      <PageTitle
        title="new repository"
        description="create a repository owned by your organization. collaboration, actions, and agents are ready out of the box."
        eyebrow={
          <Link
            href="/"
            className="text-[0.8125rem] text-kumo-inactive hover:text-kumo-default"
          >
            ← dashboard
          </Link>
        }
      />

      <form action={createRepo} className="mt-8 max-w-xl space-y-5">
        <label className="block text-[0.8125rem] text-kumo-inactive">
          name
          <input
            name="name"
            required
            pattern="[a-z0-9-_]+"
            placeholder="weave"
            className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-base px-3 py-2.5 text-[0.9375rem] text-kumo-default outline-none focus:border-kumo-contrast"
          />
        </label>

        <label className="block text-[0.8125rem] text-kumo-inactive">
          description
          <input
            name="description"
            placeholder="optional"
            className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-base px-3 py-2.5 text-[0.9375rem] text-kumo-default outline-none focus:border-kumo-contrast"
          />
        </label>

        <label className="block text-[0.8125rem] text-kumo-inactive">
          repository kind
          <select
            name="kind"
            defaultValue="code"
            className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-base px-3 py-2.5 text-[0.9375rem] text-kumo-default outline-none focus:border-kumo-contrast"
          >
            <option value="code">code — source, packages, and services</option>
            <option value="model">model — weights, cards, and evaluations</option>
            <option value="dataset">dataset — data, schemas, and lineage</option>
          </select>
          <span className="mt-1.5 block text-[0.75rem] leading-relaxed">
            Model and dataset repos route artifacts over 1 MiB through Arachne automatically; code repos use 10 MiB.
          </span>
        </label>

        <label className="block text-[0.8125rem] text-kumo-inactive">
          visibility
          <select
            name="visibility"
            defaultValue="public"
            className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-base px-3 py-2.5 text-[0.9375rem] text-kumo-default outline-none focus:border-kumo-contrast"
          >
            <option value="public">public</option>
            <option value="private">private</option>
            <option value="internal">internal</option>
          </select>
        </label>

        <Button type="submit">create repository</Button>
      </form>
    </PageFrame>
  );
}
