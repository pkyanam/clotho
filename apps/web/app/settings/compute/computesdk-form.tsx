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

function defaultUpstreamId(upstreams: Upstream[]): string {
  if (upstreams.some((u) => u.id === "e2b")) return "e2b";
  return upstreams[0]?.id ?? "";
}

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

  const [upstreamId, setUpstreamId] = useState(() =>
    defaultUpstreamId(sorted),
  );

  // Prefer explicit selection; fall back if catalog reloads without that id.
  const selected =
    sorted.find((u) => u.id === upstreamId) ??
    sorted.find((u) => u.id === defaultUpstreamId(sorted)) ??
    sorted[0];

  const fields = useMemo(() => {
    if (!selected) return [] as { name: string; required: boolean }[];
    const required = selected.required ?? [];
    const optional = selected.optional ?? [];
    return [
      ...required.map((name) => ({ name, required: true })),
      ...optional.map((name) => ({ name, required: false })),
    ];
  }, [selected]);

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
          value={selected?.id ?? ""}
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

      {/* Remount credential fields when provider changes so labels/inputs cannot stick. */}
      <div key={selected?.id ?? "none"} className="space-y-3">
        {selected?.notes ? (
          <p className="text-[0.75rem] text-kumo-inactive">{selected.notes}</p>
        ) : null}

        <div className="flex flex-wrap items-end gap-3">
          {fields.map((field) => (
            <label
              key={`${selected?.id}-${field.name}`}
              className="min-w-[200px] grow text-[0.8125rem] text-kumo-inactive"
            >
              {field.name}
              {field.required ? "" : " (optional)"}
              <input
                name={field.name}
                type="password"
                required={field.required}
                autoComplete="off"
                data-1p-ignore
                data-lpignore="true"
                placeholder={
                  configured ? "enter new value to rotate" : "paste secret"
                }
                className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default outline-none focus:border-kumo-contrast"
              />
            </label>
          ))}

          {fields.length === 0 && (
            <p className="text-[0.8125rem] text-kumo-inactive">
              this upstream uses host defaults (e.g. kubeconfig). optional
              secrets can still be set when listed.
            </p>
          )}

          <Button type="submit">
            {configured ? "save upstream" : "connect"}
          </Button>
        </div>

        <p className="text-[0.75rem] text-kumo-inactive">
          package: {selected?.pkg}. multi-provider routing uses
          priority/fallback on the bridge.
        </p>
      </div>
    </form>
  );
}
