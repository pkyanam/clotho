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
  const [upstreamId, setUpstreamId] = useState(sorted[0]?.id ?? "e2b");
  const selected = sorted.find((u) => u.id === upstreamId) ?? sorted[0];
  const fields = [
    ...(selected?.required ?? []),
    ...(selected?.optional ?? []),
  ];

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
          value={upstreamId}
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
      {selected?.notes ? (
        <p className="text-[0.75rem] text-kumo-inactive">{selected.notes}</p>
      ) : null}
      <div className="flex flex-wrap items-end gap-3">
        {fields.map((field) => {
          const required = selected?.required.includes(field) ?? false;
          return (
            <label
              key={field}
              className="min-w-[200px] grow text-[0.8125rem] text-kumo-inactive"
            >
              {field}
              {required ? "" : " (optional)"}
              <input
                name={field}
                type="password"
                required={required}
                autoComplete="off"
                placeholder={
                  configured ? "enter new value to rotate" : "paste secret"
                }
                className="mt-1.5 block w-full border border-kumo-hairline bg-kumo-canvas px-3 py-2 text-[0.875rem] text-kumo-default outline-none focus:border-kumo-contrast"
              />
            </label>
          );
        })}
        {fields.length === 0 && (
          <p className="text-[0.8125rem] text-kumo-inactive">
            this upstream uses host defaults (e.g. kubeconfig). optional secrets
            can still be set above when listed.
          </p>
        )}
        <Button type="submit">
          {configured ? "save upstream" : "connect"}
        </Button>
      </div>
      <p className="text-[0.75rem] text-kumo-inactive">
        package: {selected?.pkg}. multi-provider routing uses priority/fallback
        on the bridge.
      </p>
    </form>
  );
}
