"use client";

import { useRouter } from "next/navigation";
import { Button } from "@cloudflare/kumo";

export function MarkAllReadButton() {
  const router = useRouter();

  return (
    <Button
      type="button"
      variant="secondary"
      onClick={async () => {
        await fetch("/api/notifications/mark-read", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ all: true }),
        });
        router.refresh();
      }}
    >
      mark all read
    </Button>
  );
}
