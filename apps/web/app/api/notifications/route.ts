import { NextResponse } from "next/server";
import { ClothoApiError } from "@clotho/sdk-js";

import { api } from "src/lib/api";

export async function GET(request: Request) {
  const url = new URL(request.url);
  const unread = url.searchParams.get("unread") === "true";
  const limit = Number(url.searchParams.get("limit") ?? "20");
  try {
    const result = await (await api()).notifications({
      unread: unread || undefined,
      limit: Number.isFinite(limit) ? limit : 20,
    });
    return NextResponse.json(result);
  } catch (error) {
    // Stale gateway images (pre–Slice D) return 404 for this path — degrade
    // quietly so the header bell does not spam 500s every poll.
    if (error instanceof ClothoApiError && error.status === 404) {
      return NextResponse.json({ notifications: [], unread_count: 0 });
    }
    const status = error instanceof ClothoApiError ? error.status : 502;
    const message =
      error instanceof Error ? error.message : "notifications unavailable";
    return NextResponse.json({ error: message }, { status });
  }
}
