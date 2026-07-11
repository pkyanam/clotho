"use client";

import Link from "next/link";
import { Button } from "@cloudflare/kumo";

export default function Error({
  error,
  reset,
}: {
  error: Error;
  reset: () => void;
}) {
  return (
    <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6">
      <div className="mx-auto max-w-lg border border-kumo-hairline bg-kumo-base px-6 py-12 text-center">
        <p className="text-[0.8125rem] uppercase tracking-wide text-kumo-inactive">
          something snapped a thread
        </p>
        <h1 className="mt-3 text-[1.25rem] leading-tight text-kumo-default">
          this page could not load
        </h1>
        <p className="mx-auto mt-3 max-w-md break-words text-[0.9375rem] leading-relaxed text-kumo-inactive">
          {error.message || "an upstream service is unreachable."}
        </p>
        <div className="mt-8 flex items-center justify-center gap-3">
          <Button variant="primary" onClick={reset}>
            retry
          </Button>
          <Link
            href="/"
            className="text-[0.875rem] text-kumo-inactive hover:text-kumo-default"
          >
            back to dashboard
          </Link>
        </div>
      </div>
    </div>
  );
}
