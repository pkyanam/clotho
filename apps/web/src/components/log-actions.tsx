"use client";

import { Button } from "@cloudflare/kumo";

export function LogActions({
  text,
  filename,
}: {
  text: string;
  filename: string;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Button
        type="button"
        onClick={() => {
          void navigator.clipboard.writeText(text);
        }}
      >
        copy
      </Button>
      <Button
        type="button"
        onClick={() => {
          const blob = new Blob([text], { type: "text/plain;charset=utf-8" });
          const url = URL.createObjectURL(blob);
          const link = document.createElement("a");
          link.href = url;
          link.download = filename;
          link.click();
          URL.revokeObjectURL(url);
        }}
      >
        download
      </Button>
    </div>
  );
}
