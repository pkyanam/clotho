"use client";

import { Button } from "@cloudflare/kumo";

export default function Error({
  error,
  reset,
}: {
  error: Error;
  reset: () => void;
}) {
  return (
    <div className="mx-auto max-w-6xl px-6 py-24">
      <h1 className="text-lg">something snapped a thread</h1>
      <p className="mt-3 max-w-xl text-sm text-kumo-subtle">
        {error.message || "an upstream service is unreachable."}
      </p>
      <div className="mt-6">
        <Button variant="primary" onClick={reset}>
          retry
        </Button>
      </div>
    </div>
  );
}
