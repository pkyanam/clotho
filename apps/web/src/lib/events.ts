import type { ActivityEvent } from "@clotho/sdk-js";

export interface FormattedEvent {
  text: string;
  /** Console route the event points at, when the payload names one. */
  href?: string;
}

/** Human-readable line + optional destination for a control-plane event. */
export function formatEvent(ev: ActivityEvent): FormattedEvent {
  const payload = (ev.payload ?? {}) as Record<string, unknown>;
  const repoName = typeof payload.repo_name === "string" ? payload.repo_name : "";
  const orgName = typeof payload.org_name === "string" ? payload.org_name : "";

  switch (ev.event_type) {
    case "repo.created":
      return {
        text: `repository ${repoName || "(unnamed)"} created`,
        href: repoName ? `/repos/${repoName}` : undefined,
      };
    case "org.created":
      return {
        text: `organization ${orgName || "(unnamed)"} created`,
        href: orgName ? `/orgs/${orgName}` : undefined,
      };
    case "secret.created":
      return {
        text: `secret ${String(payload.name ?? "")} created`,
        href: "/settings/secrets",
      };
    case "secret.updated":
      return {
        text: `secret ${String(payload.name ?? "")} rotated`,
        href: "/settings/secrets",
      };
    case "secret.deleted":
      return {
        text: `secret ${String(payload.name ?? "")} deleted`,
        href: "/settings/secrets",
      };
    case "provider.connected":
      return {
        text: `provider ${String(payload.provider ?? "")} connected`,
        href: "/settings/compute",
      };
    case "provider.disconnected":
      return {
        text: `provider ${String(payload.provider ?? "")} disconnected`,
        href: "/settings/compute",
      };
    default:
      return { text: ev.event_type.replace(/\./g, " ") };
  }
}

export function isoToMillis(iso: string): number {
  const t = Date.parse(iso);
  return Number.isFinite(t) ? t : 0;
}
