import { api } from "src/lib/api";
import { ClothoApiError } from "@clotho/sdk-js";

export const dynamic = "force-dynamic";

async function proxyReleaseFile(
  request: Request,
  context: {
    params: Promise<{ name: string; version: string; path: string[] }>;
  },
) {
  const { name, version, path } = await context.params;
  let upstream: Response;
  try {
    upstream = await (await api()).downloadReleaseFile(
      name,
      version,
      path.join("/"),
      { head: request.method === "HEAD" },
    );
  } catch (error) {
    if (error instanceof ClothoApiError) {
      return Response.json({ error: error.message }, { status: error.status });
    }
    throw error;
  }
  const headers = new Headers();
  for (const key of [
    "content-type",
    "content-length",
    "etag",
    "cache-control",
    "x-clotho-release-version",
    "x-clotho-commit-id",
    "x-clotho-manifest-sha256",
    "x-clotho-arachne-hash",
  ]) {
    const value = upstream.headers.get(key);
    if (value) headers.set(key, value);
  }
  return new Response(request.method === "HEAD" ? null : upstream.body, {
    status: upstream.status,
    headers,
  });
}

export async function GET(
  request: Request,
  context: {
    params: Promise<{ name: string; version: string; path: string[] }>;
  },
) {
  return proxyReleaseFile(request, context);
}

export async function HEAD(
  request: Request,
  context: {
    params: Promise<{ name: string; version: string; path: string[] }>;
  },
) {
  return proxyReleaseFile(request, context);
}
