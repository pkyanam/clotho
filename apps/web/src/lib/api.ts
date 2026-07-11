import { auth } from "@clerk/nextjs/server";
import { ClothoClient } from "@clotho/sdk-js";

export {
  browserApiUrl,
  cloneUrl,
  formatBytes,
  publicCloneUrl,
  shortId,
  timeAgo,
} from "./api-shared";

/**
 * Gateway base URLs. Server components talk to the gateway directly
 * (CLOTHO_API_URL); client components (the polling presence panel) go
 * through the browser, so they need a browser-reachable URL.
 */
export const serverApiUrl =
  process.env.CLOTHO_API_URL ?? "http://localhost:8080";

function clerkConfigured(): boolean {
  return Boolean(
    process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY?.trim() ||
      process.env.CLERK_PUBLISHABLE_KEY?.trim(),
  );
}

/**
 * Server-side client. Prefer a verified Clerk session JWT when Clerk is
 * configured (never expose CLERK_SECRET_KEY to the browser). Fall back to
 * CLOTHO_TOKEN / CLOTHO_API_TOKEN for local bootstrap (ADR-0015).
 */
export async function api(): Promise<ClothoClient> {
  let token =
    process.env.CLOTHO_TOKEN ?? process.env.CLOTHO_API_TOKEN ?? undefined;

  if (clerkConfigured()) {
    try {
      const session = await auth();
      const clerkToken = await session.getToken();
      if (clerkToken) {
        token = clerkToken;
      }
    } catch {
      // Clerk middleware may be absent in bootstrap/dev; keep env token.
    }
  }

  return new ClothoClient({
    baseUrl: serverApiUrl,
    token,
    fetch: (input, init) => fetch(input, { ...init, cache: "no-store" }),
  });
}
