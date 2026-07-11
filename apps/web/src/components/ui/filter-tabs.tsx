import Link from "next/link";

/** One segmented filter link (open / closed / all) for list pages. */
export function FilterTab({
  href,
  label,
  current,
}: {
  href: string;
  label: string;
  current: boolean;
}) {
  return (
    <Link
      href={href}
      className={`border px-3 py-1.5 text-[0.8125rem] transition-colors ${
        current
          ? "border-kumo-hairline bg-kumo-elevated text-kumo-default"
          : "border-transparent text-kumo-inactive hover:text-kumo-default"
      }`}
    >
      {label}
    </Link>
  );
}
