import Link from "next/link";

export default function NotFound() {
  return (
    <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6">
      <div className="mx-auto max-w-lg border border-kumo-hairline bg-kumo-base px-6 py-12 text-center">
        <p className="text-[0.8125rem] uppercase tracking-wide text-kumo-inactive">
          404
        </p>
        <h1 className="mt-3 text-[1.25rem] leading-tight text-kumo-default">
          no such thread in the loom
        </h1>
        <p className="mx-auto mt-3 max-w-md text-[0.9375rem] leading-relaxed text-kumo-inactive">
          the page you followed does not exist, or the repository behind it has
          moved.
        </p>
        <div className="mt-8 flex items-center justify-center gap-4 text-[0.875rem]">
          <Link
            href="/"
            className="border border-kumo-hairline px-4 py-2 transition-colors hover:bg-kumo-elevated"
          >
            dashboard
          </Link>
          <Link
            href="/repos"
            className="text-kumo-inactive hover:text-kumo-default"
          >
            all repositories
          </Link>
        </div>
      </div>
    </div>
  );
}
