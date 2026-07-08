import Link from "next/link";

type RepoSection =
  | "code"
  | "pulls"
  | "issues"
  | "checks"
  | "agents"
  | "storage"
  | "insights"
  | "settings";

const sections: Array<{ key: RepoSection; label: string; href: string }> = [
  { key: "code", label: "code", href: "" },
  { key: "pulls", label: "pull requests", href: "/pulls" },
  { key: "issues", label: "issues", href: "/issues" },
  { key: "checks", label: "checks", href: "/checks" },
  { key: "agents", label: "agents", href: "/agents" },
  { key: "storage", label: "storage", href: "/storage" },
  { key: "insights", label: "insights", href: "/insights" },
  { key: "settings", label: "settings", href: "/settings" },
];

export function RepoNav({
  name,
  active,
}: {
  name: string;
  active: RepoSection;
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
      <nav className="flex items-center gap-5 overflow-x-auto">
        {sections.map((section) => (
          <span key={section.key}>
            {tab(
              `/repos/${name}${section.href}`,
              section.label,
              active === section.key,
            )}
          </span>
        ))}
      </nav>
    </div>
  );
}
