import Link from "next/link";

type RepoSection =
  | "code"
  | "commits"
  | "branches"
  | "pulls"
  | "issues"
  | "actions"
  | "agents"
  | "storage"
  | "insights"
  | "settings";

const sections: Array<{ key: RepoSection; label: string; href: string }> = [
  { key: "code", label: "code", href: "" },
  { key: "commits", label: "commits", href: "/commits" },
  { key: "branches", label: "branches", href: "/branches" },
  { key: "pulls", label: "pull requests", href: "/pulls" },
  { key: "issues", label: "issues", href: "/issues" },
  { key: "actions", label: "actions", href: "/actions" },
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
  return (
    <div className="border-b border-kumo-hairline">
      <div className="flex flex-wrap items-baseline gap-2 pb-3 text-[0.8125rem] text-kumo-inactive">
        <Link href="/repos" className="hover:text-kumo-default">
          repos
        </Link>
        <span>/</span>
        <span className="text-kumo-default">{name}</span>
      </div>
      <nav className="flex items-center gap-1 overflow-x-auto pb-2">
        {sections.map((section) => {
          const current = active === section.key;
          return (
            <Link
              key={section.key}
              href={`/repos/${name}${section.href}`}
              className={`shrink-0 rounded-md px-3 py-1.5 text-[0.8125rem] transition-colors ${
                current
                  ? "bg-accent-surface font-medium text-accent-strong"
                  : "text-kumo-inactive hover:bg-kumo-elevated hover:text-kumo-default"
              }`}
            >
              {section.label}
            </Link>
          );
        })}
      </nav>
    </div>
  );
}
