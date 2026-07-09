"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";

const NAV = [
  { href: "/", label: "dashboard" },
  { href: "/repos", label: "repos" },
  { href: "/agents", label: "agents" },
  { href: "/activity", label: "activity" },
  { href: "/settings", label: "settings" },
] as const;

function isActive(pathname: string, href: string): boolean {
  if (href === "/") return pathname === "/";
  return pathname === href || pathname.startsWith(`${href}/`);
}

export function AppShell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const [mobileOpen, setMobileOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);

  // Close the mobile drawer when the route changes by remounting its open state
  // via pathname key on the drawer container (avoids setState-in-effect).
  const drawerKey = pathname;

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
      if (e.key === "Escape") {
        setPaletteOpen(false);
        setMobileOpen(false);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="flex min-h-full flex-col">
      <header className="sticky top-0 z-30 border-b border-kumo-hairline bg-kumo-canvas/80 backdrop-blur-xl">
        <div className="mx-auto flex h-14 max-w-7xl items-center gap-4 px-4 sm:px-6">
          <button
            type="button"
            className="flex h-9 w-9 items-center justify-center border border-kumo-hairline text-sm md:hidden"
            aria-label="open navigation"
            onClick={() => setMobileOpen((v) => !v)}
          >
            ☰
          </button>

          <Link
            href="/"
            className="text-[0.9375rem] font-medium tracking-wide text-kumo-default"
          >
            clotho
          </Link>

          <nav className="hidden items-center gap-1 md:flex">
            {NAV.map((item) => (
              <Link
                key={item.href}
                href={item.href}
                className={`px-3 py-1.5 text-[0.8125rem] transition-colors ${
                  isActive(pathname, item.href)
                    ? "text-kumo-default"
                    : "text-kumo-inactive hover:text-kumo-default"
                }`}
              >
                {item.label}
              </Link>
            ))}
          </nav>

          <div className="ml-auto flex items-center gap-2">
            <button
              type="button"
              onClick={() => setPaletteOpen(true)}
              className="hidden h-9 items-center gap-2 border border-kumo-hairline px-3 text-[0.8125rem] text-kumo-inactive transition-colors hover:border-kumo-contrast hover:text-kumo-default sm:flex"
              aria-label="open command palette"
            >
              <span>search</span>
              <kbd className="border border-kumo-hairline px-1.5 py-0.5 text-[0.6875rem] text-kumo-inactive">
                ⌘k
              </kbd>
            </button>
            <Link
              href="/settings/compute"
              className="hidden text-[0.8125rem] text-kumo-inactive hover:text-kumo-default lg:inline"
            >
              compute
            </Link>
            <Link
              href="/settings/secrets"
              className="hidden text-[0.8125rem] text-kumo-inactive hover:text-kumo-default lg:inline"
            >
              secrets
            </Link>
          </div>
        </div>

        {mobileOpen && (
          <nav
            key={drawerKey}
            className="border-t border-kumo-hairline px-4 py-3 md:hidden"
          >
            <ul className="space-y-1">
              {NAV.map((item) => (
                <li key={item.href}>
                  <Link
                    href={item.href}
                    onClick={() => setMobileOpen(false)}
                    className={`block px-3 py-2.5 text-sm ${
                      isActive(pathname, item.href)
                        ? "bg-kumo-elevated text-kumo-default"
                        : "text-kumo-inactive"
                    }`}
                  >
                    {item.label}
                  </Link>
                </li>
              ))}
              <li>
                <button
                  type="button"
                  className="block w-full px-3 py-2.5 text-left text-sm text-kumo-inactive"
                  onClick={() => {
                    setMobileOpen(false);
                    setPaletteOpen(true);
                  }}
                >
                  search · ⌘k
                </button>
              </li>
            </ul>
          </nav>
        )}
      </header>

      <main className="flex-1">{children}</main>

      {paletteOpen && (
        <CommandPalette onClose={() => setPaletteOpen(false)} />
      )}
    </div>
  );
}

function CommandPalette({ onClose }: { onClose: () => void }) {
  const [query, setQuery] = useState("");
  const commands = [
    { href: "/", label: "go to dashboard", keywords: "home" },
    { href: "/repos/new", label: "create repository", keywords: "new repo" },
    { href: "/settings/compute", label: "compute providers", keywords: "daytona connect" },
    { href: "/settings/secrets", label: "secrets", keywords: "api key credentials" },
    { href: "/settings", label: "settings hub", keywords: "account org" },
    { href: "/agents", label: "agents", keywords: "sessions identity" },
    { href: "/activity", label: "activity", keywords: "feed events" },
    { href: "/repos", label: "all repositories", keywords: "repos list" },
  ].filter((c) => {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return (
      c.label.includes(q) ||
      c.keywords.includes(q) ||
      c.href.includes(q)
    );
  });

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/70 px-4 pt-[12vh]"
      role="dialog"
      aria-modal="true"
      aria-label="command palette"
      onClick={onClose}
    >
      <div
        className="w-full max-w-lg border border-kumo-hairline bg-kumo-base shadow-none"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="border-b border-kumo-hairline px-4 py-3">
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="jump to…"
            className="w-full bg-transparent text-[0.9375rem] text-kumo-default outline-none placeholder:text-kumo-placeholder"
            aria-label="command search"
          />
        </div>
        <ul className="max-h-72 overflow-y-auto py-1">
          {commands.length === 0 ? (
            <li className="px-4 py-6 text-sm text-kumo-inactive">no matches</li>
          ) : (
            commands.map((c) => (
              <li key={c.href + c.label}>
                <Link
                  href={c.href}
                  onClick={onClose}
                  className="flex items-center justify-between px-4 py-2.5 text-sm transition-colors hover:bg-kumo-elevated"
                >
                  <span>{c.label}</span>
                  <span className="text-[0.75rem] text-kumo-inactive">{c.href}</span>
                </Link>
              </li>
            ))
          )}
        </ul>
        <div className="border-t border-kumo-hairline px-4 py-2 text-[0.75rem] text-kumo-inactive">
          esc to close · enter to open
        </div>
      </div>
    </div>
  );
}
