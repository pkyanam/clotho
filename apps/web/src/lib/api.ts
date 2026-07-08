import { ClothoClient } from "@clotho/sdk-js";

/**
 * Gateway base URLs. Server components talk to the gateway directly
 * (CLOTHO_API_URL); client components (the polling presence panel) go
 * through the browser, so they need a browser-reachable URL.
 */
export const serverApiUrl =
  process.env.CLOTHO_API_URL ?? "http://localhost:8080";

export const browserApiUrl =
  process.env.NEXT_PUBLIC_CLOTHO_API_URL ?? "http://localhost:8080";

/** Server-side client. Never cache: the repo graph moves under agents. */
export function api(): ClothoClient {
  return new ClothoClient({
    baseUrl: serverApiUrl,
    fetch: (input, init) => fetch(input, { ...init, cache: "no-store" }),
  });
}

export function timeAgo(millis: number): string {
  const seconds = Math.max(0, Math.floor((Date.now() - millis) / 1000));
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

export function shortId(id: string): string {
  return id.slice(0, 12);
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} b`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} kib`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} mib`;
}

export function cloneUrl(owner: string, name: string): string {
  const base = process.env.NEXT_PUBLIC_CLOTHO_GIT_URL ?? "http://localhost:13000";
  return `${base.replace(/\/$/, "")}/${owner}/${name}.git`;
}
