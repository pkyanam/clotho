/** Browser-reachable gateway URL for client-side polling. */
export const browserApiUrl =
  process.env.NEXT_PUBLIC_CLOTHO_API_URL ?? "http://localhost:8080";

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

/**
 * Public clone URL shown in the product UI.
 * Prefer NEXT_PUBLIC_CLOTHO_GIT_URL when set; otherwise a Clotho-facing host
 * (never internal docker service names).
 */
export function cloneUrl(owner: string, name: string): string {
  const base =
    process.env.NEXT_PUBLIC_CLOTHO_GIT_URL ??
    process.env.NEXT_PUBLIC_CLOTHO_PUBLIC_GIT_URL ??
    "http://localhost:13000";
  const cleaned = base.replace(/\/$/, "");
  const publicBase = cleaned
    .replace(/\/\/forgejo(?::\d+)?/, "//localhost:13000")
    .replace(/\/\/gitea(?::\d+)?/, "//localhost:13000");
  return `${publicBase}/${owner}/${name}.git`;
}

/** Sanitize a clone URL from the API so internal service hosts never render. */
export function publicCloneUrl(url: string, owner: string, name: string): string {
  if (!url || /forgejo|gitea|:3000\//i.test(url)) {
    return cloneUrl(owner, name);
  }
  return url;
}

