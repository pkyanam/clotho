import Link from "next/link";

const items = [
  { href: "/settings", label: "overview", exact: true },
  { href: "/settings/compute", label: "compute" },
  { href: "/settings/secrets", label: "secrets" },
] as const;

export function SettingsNav({ active }: { active: "overview" | "compute" | "secrets" }) {
  return (
    <nav className="flex flex-wrap gap-1 border-b border-kumo-hairline pb-3">
      {items.map((item) => {
        const current =
          active === "overview"
            ? item.label === "overview"
            : item.label === active;
        return (
          <Link
            key={item.href}
            href={item.href}
            className={`px-3 py-1.5 text-[0.8125rem] transition-colors ${
              current
                ? "border border-kumo-hairline bg-kumo-elevated text-kumo-default"
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
