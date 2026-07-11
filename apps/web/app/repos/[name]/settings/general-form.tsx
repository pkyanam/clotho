"use client";

import { Button, Input, Select } from "@cloudflare/kumo";

import { updateRepoSettings } from "./actions";

export function GeneralForm({
  repo,
  description,
  visibility,
  defaultBranch,
}: {
  repo: string;
  description: string;
  visibility: string;
  defaultBranch: string;
}) {
  return (
    <form action={updateRepoSettings.bind(null, repo)} className="space-y-4">
      <label className="block text-[0.8125rem] text-kumo-inactive">
        description
        <Input
          name="description"
          defaultValue={description}
          className="mt-1 w-full"
          placeholder="short summary of this repository"
        />
      </label>
      <label className="block text-[0.8125rem] text-kumo-inactive">
        visibility
        <Select name="visibility" defaultValue={visibility} className="mt-1 w-full">
          <option value="public">public</option>
          <option value="private">private</option>
          <option value="internal">internal</option>
        </Select>
      </label>
      <label className="block text-[0.8125rem] text-kumo-inactive">
        default branch
        <Input
          name="default_branch"
          defaultValue={defaultBranch}
          className="mt-1 w-full"
        />
      </label>
      <Button type="submit" variant="outline">
        save changes
      </Button>
    </form>
  );
}
