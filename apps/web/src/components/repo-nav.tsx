import Link from "next/link";

/**
 * Repo-level navigation: breadcrumb back to the repo list plus the repo's
 * sections. Issues stay in Forgejo for the prototype — the link is honest
 * about leaving Clotho's shell.
 */
export function RepoNav({
  name,
  active,
  forgejoUrl,
}: {
  name: string;
  active: "files" | "pulls";
  forgejoUrl?: string;
}) {
  const tab = (href: string, label: string, current: boolean) => (
    <Link
      href={href}
      className={`border-b pb-2 text-xs transition-colors ${
        current
          ? "border-kumo-contrast text-kumo-default"
          : "border-transparent text-kumo-inactive hover:text-kumo-default"
      }`}
    >
      {label}
    </Link>
  );

  return (
    <div className="border-b border-kumo-hairline">
      <div className="flex items-baseline gap-2 pb-3 text-xs text-kumo-inactive">
        <Link href="/" className="hover:text-kumo-default">
          repos
        </Link>
        <span>/</span>
        <span className="text-kumo-default">{name}</span>
      </div>
      <nav className="flex items-center gap-5">
        {tab(`/repos/${name}`, "files", active === "files")}
        {tab(`/repos/${name}/pulls`, "pull requests", active === "pulls")}
        {forgejoUrl && (
          <a
            href={forgejoUrl}
            className="ml-auto border-b border-transparent pb-2 text-xs text-kumo-inactive hover:text-kumo-default"
          >
            issues in forgejo ↗
          </a>
        )}
      </nav>
    </div>
  );
}
