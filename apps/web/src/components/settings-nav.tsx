import Link from "next/link";

const items = [
  { href: "/settings", label: "overview", key: "overview" as const },
  {
    href: "/settings/appearance",
    label: "appearance",
    key: "appearance" as const,
  },
  { href: "/settings/compute", label: "compute", key: "compute" as const },
  { href: "/settings/network", label: "network", key: "network" as const },
  { href: "/settings/secrets", label: "secrets", key: "secrets" as const },
] as const;

export function SettingsNav({
  active,
}: {
  active: "overview" | "appearance" | "compute" | "network" | "secrets";
}) {
  return (
    <nav className="flex flex-wrap gap-1 border-b border-kumo-hairline pb-3">
      {items.map((item) => {
        const current = item.key === active;
        return (
          <Link
            key={item.href}
            href={item.href}
            aria-current={current ? "page" : undefined}
            className={`rounded-md px-3 py-1.5 text-[0.8125rem] transition-colors ${
              current
                ? "clotho-active-nav font-medium"
                : "text-kumo-inactive hover:text-kumo-default"
            }`}
          >
            {item.label}
          </Link>
        );
      })}
    </nav>
  );
}
