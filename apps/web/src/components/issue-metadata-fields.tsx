"use client";

import type { Label, OrgMembership } from "@clotho/sdk-js";

export function IssueMetadataFields({
  labels,
  members,
  defaultLabels = [],
  defaultAssignees = [],
}: {
  labels: Label[];
  members: OrgMembership[];
  defaultLabels?: string[];
  defaultAssignees?: string[];
}) {
  return (
    <div className="space-y-5">
      {labels.length > 0 && (
        <fieldset>
          <legend className="text-[0.8125rem] text-kumo-inactive">labels</legend>
          <div className="mt-2 flex flex-wrap gap-3">
            {labels.map((label) => (
              <label
                key={label.id}
                className="flex items-center gap-2 text-[0.8125rem] text-kumo-default"
              >
                <input
                  type="checkbox"
                  name="labels"
                  value={label.name}
                  defaultChecked={defaultLabels.includes(label.name)}
                  className="border border-kumo-hairline"
                />
                {label.name}
              </label>
            ))}
          </div>
        </fieldset>
      )}

      {members.length > 0 && (
        <label className="block text-[0.8125rem] text-kumo-inactive">
          assignee
          <select
            name="assignee"
            defaultValue={defaultAssignees[0] ?? ""}
            className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-base px-3 py-2.5 text-[0.9375rem] text-kumo-default outline-none focus:border-kumo-contrast"
          >
            <option value="">unassigned</option>
            {members.map((m) => (
              <option key={m.user_id} value={m.user_name}>
                {m.user_display_name || m.user_name}
              </option>
            ))}
          </select>
        </label>
      )}
    </div>
  );
}
