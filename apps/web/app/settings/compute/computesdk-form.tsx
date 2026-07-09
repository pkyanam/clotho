"use client";

import { useMemo, useState } from "react";
import { Button } from "@cloudflare/kumo";

import { connectProvider } from "./actions";

export type Upstream = {
  id: string;
  name: string;
  pkg: string;
  required: string[];
  optional?: string[];
  notes?: string;
};

/**
 * One connect form for every ComputeSDK upstream.
 *
 * Each upstream gets its own field panel. Switching the dropdown only toggles
 * which panel is visible/enabled — we never reuse a single field list, so
 * labels cannot stick on E2B_API_KEY / AGENTUITY_SDK_KEY across selections.
 */
export function ComputesdkConnectForm({
  org,
  upstreams,
  configured,
}: {
  org: string;
  upstreams: Upstream[];
  configured: boolean;
}) {
  const sorted = useMemo(
    () => [...upstreams].sort((a, b) => a.name.localeCompare(b.name)),
    [upstreams],
  );

  const initialId =
    sorted.find((u) => u.id === "e2b")?.id ?? sorted[0]?.id ?? "";
  const [upstreamId, setUpstreamId] = useState(initialId);

  const activeId = sorted.some((u) => u.id === upstreamId)
    ? upstreamId
    : initialId;
  const active = sorted.find((u) => u.id === activeId);

  if (sorted.length === 0) {
    return (
      <p className="mt-3 text-[0.8125rem] text-kumo-inactive">
        upstream catalog unavailable — set secrets via settings → secrets
        using env names from{" "}
        <a
          href="https://docs.computesdk.com/providers"
          className="underline hover:text-kumo-default"
          target="_blank"
          rel="noreferrer"
        >
          docs.computesdk.com/providers
        </a>
        .
      </p>
    );
  }

  return (
    <form
      action={connectProvider.bind(null, "computesdk")}
      className="mt-3 space-y-3"
    >
      <input type="hidden" name="org" value={org} />

      <label className="block text-[0.8125rem] text-kumo-inactive">
        upstream provider
        <select
          name="upstream"
          value={activeId}
          onChange={(e) => setUpstreamId(e.target.value)}
          className="mt-1.5 block w-full max-w-md border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default outline-none focus:border-kumo-contrast"
        >
          {sorted.map((u) => (
            <option key={u.id} value={u.id}>
              {u.name} ({u.id})
            </option>
          ))}
        </select>
      </label>

      {active && (
        <p className="text-[0.8125rem] text-kumo-default">
          connecting: <strong>{active.name}</strong>
          <span className="text-kumo-inactive"> · {active.pkg}</span>
        </p>
      )}

      {sorted.map((u) => {
        const isActive = u.id === activeId;
        const required = u.required ?? [];
        const optional = u.optional ?? [];
        const allFields = [
          ...required.map((name) => ({ name, required: true as const })),
          ...optional.map((name) => ({ name, required: false as const })),
        ];

        return (
          <div
            key={u.id}
            // Keep inactive panels out of the accessibility tree and layout.
            hidden={!isActive}
            aria-hidden={!isActive}
            className="space-y-3 border border-kumo-hairline bg-kumo-canvas/40 p-4"
          >
            {u.notes ? (
              <p className="text-[0.75rem] text-kumo-inactive">{u.notes}</p>
            ) : null}

            <div className="flex flex-wrap items-end gap-3">
              {allFields.map((field) => (
                <label
                  key={`${u.id}-${field.name}`}
                  className="min-w-[200px] grow text-[0.8125rem] text-kumo-inactive"
                >
                  {field.name}
                  {field.required ? "" : " (optional)"}
                  <input
                    // Only the active panel submits values.
                    name={isActive ? field.name : undefined}
                    type="password"
                    required={isActive && field.required}
                    disabled={!isActive}
                    autoComplete="off"
                    data-1p-ignore
                    data-lpignore="true"
                    placeholder={
                      configured ? "enter new value to rotate" : "paste secret"
                    }
                    className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-base px-3 py-2 text-[0.875rem] text-kumo-default outline-none focus:border-kumo-contrast disabled:opacity-40"
                  />
                </label>
              ))}

              {allFields.length === 0 && isActive && (
                <p className="text-[0.8125rem] text-kumo-inactive">
                  this upstream uses host defaults (e.g. kubeconfig). optional
                  secrets can still be set when listed.
                </p>
              )}
            </div>

            {isActive && (
              <p className="text-[0.75rem] text-kumo-inactive">
                {required.length > 0
                  ? `required: ${required.join(", ")}`
                  : "no required secrets"}
                {optional.length > 0
                  ? ` · optional: ${optional.join(", ")}`
                  : ""}
              </p>
            )}
          </div>
        );
      })}

      <Button type="submit">{configured ? "save upstream" : "connect"}</Button>
    </form>
  );
}
