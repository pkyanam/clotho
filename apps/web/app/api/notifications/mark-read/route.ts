import { NextResponse } from "next/server";
import { ClothoApiError } from "@clotho/sdk-js";

import { api } from "src/lib/api";

export async function POST(request: Request) {
  const body = (await request.json()) as { all?: boolean; ids?: number[] };
  try {
    await (await api()).markNotificationsRead({
      all: body.all ?? false,
      ids: body.ids,
    });
    return NextResponse.json({ ok: true });
  } catch (error) {
    if (error instanceof ClothoApiError && error.status === 404) {
      return NextResponse.json({ ok: true, skipped: true });
    }
    const status = error instanceof ClothoApiError ? error.status : 502;
    const message =
      error instanceof Error ? error.message : "mark-read failed";
    return NextResponse.json({ error: message }, { status });
  }
}
