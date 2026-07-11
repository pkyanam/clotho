"use client";

import { Button } from "@cloudflare/kumo";

import { updateMergePolicy } from "./actions";

export function MergePolicyForm({
  repo,
  policy,
}: {
  repo: string;
  policy: {
    require_passing_actions: boolean;
    block_merge_when_conflicted: boolean;
    require_review_approvals: number;
    protect_default_branch: boolean;
  };
}) {
  return (
    <form action={updateMergePolicy.bind(null, repo)} className="space-y-4">
      <label className="flex items-center gap-2 text-[0.875rem] text-kumo-default">
        <input
          type="checkbox"
          name="require_passing_actions"
          defaultChecked={policy.require_passing_actions}
          className="size-4"
        />
        require passing actions on the pull request head
      </label>
      <label className="flex items-center gap-2 text-[0.875rem] text-kumo-default">
        <input
          type="checkbox"
          name="block_merge_when_conflicted"
          defaultChecked={policy.block_merge_when_conflicted}
          className="size-4"
        />
        block merge when the pull request has conflicts
      </label>
      <label className="block text-[0.8125rem] text-kumo-inactive">
        required approving reviews
        <input
          type="number"
          name="require_review_approvals"
          min={0}
          defaultValue={policy.require_review_approvals}
          className="mt-1 block w-24 border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default"
        />
      </label>
      <label className="flex items-center gap-2 text-[0.875rem] text-kumo-default">
        <input
          type="checkbox"
          name="protect_default_branch"
          defaultChecked={policy.protect_default_branch}
          className="size-4"
        />
        protect default branch (direct-push rules are a follow-up)
      </label>
      <p className="text-[0.8125rem] leading-relaxed text-kumo-inactive">
        full branch protection rules (required reviewers per path, push
        restrictions) are not implemented yet — these gates apply at merge time
        only.
      </p>
      <Button type="submit" variant="outline">
        save merge policy
      </Button>
    </form>
  );
}
