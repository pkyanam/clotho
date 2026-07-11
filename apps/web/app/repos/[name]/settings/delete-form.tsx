"use client";

import { Button, Input } from "@cloudflare/kumo";

import { deleteRepo } from "./actions";

export function DeleteForm({ repo }: { repo: string }) {
  return (
    <form action={deleteRepo.bind(null, repo)} className="flex flex-wrap items-end gap-3">
      <label className="min-w-0 flex-1 text-[0.8125rem] text-kumo-inactive">
        type <code className="text-kumo-default">{repo}</code> to confirm
        <Input
          name="confirm"
          className="mt-1 w-full"
          placeholder={repo}
          autoComplete="off"
        />
      </label>
      <Button type="submit" variant="outline">
        delete
      </Button>
    </form>
  );
}
