import http from "node:http";
import { randomUUID } from "node:crypto";
import { Storage } from "@storagesdk/core";

export const catalog = [
  { id: "s3", name: "Amazon S3 / compatible", required: ["bucket"], capabilities: ["objects", "signed-urls", "snapshots", "forks"] },
  { id: "minio", name: "MinIO", required: ["bucket", "endpoint", "accessKeyId", "secretAccessKey"], capabilities: ["objects", "signed-urls", "snapshots", "forks"] },
  { id: "r2", name: "Cloudflare R2", required: ["bucket", "accountId", "accessKeyId", "secretAccessKey"], capabilities: ["objects", "signed-urls", "snapshots", "forks"] },
  { id: "fs", name: "Local filesystem", required: ["root", "folder"], capabilities: ["objects", "snapshots", "forks"] },
];

async function adapterFor(provider, config) {
  switch (provider) {
    case "s3": {
      const { s3 } = await import("@storagesdk/adapters/s3");
      return s3(config);
    }
    case "minio": {
      const { minio } = await import("@storagesdk/adapters/minio");
      return minio(config);
    }
    case "r2": {
      const { r2 } = await import("@storagesdk/adapters/r2");
      return r2(config);
    }
    case "fs": {
      const { fs } = await import("@storagesdk/adapters/fs");
      return fs(config);
    }
    default:
      throw Object.assign(new Error(`unsupported StorageSDK provider ${provider}`), { status: 400 });
  }
}

async function storageFor(body) {
  if (!body?.provider || !body?.config) {
    throw Object.assign(new Error("provider and config are required"), { status: 400 });
  }
  return new Storage({ adapter: await adapterFor(body.provider, body.config) });
}

async function readJson(req) {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  if (chunks.length === 0) return {};
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw Object.assign(new Error("request body must be valid JSON"), { status: 400 });
  }
}

function json(res, status, body) {
  const encoded = JSON.stringify(body);
  res.writeHead(status, { "content-type": "application/json", "content-length": Buffer.byteLength(encoded) });
  res.end(encoded);
}

function authorized(req) {
  const token = process.env.CLOTHO_STORAGE_SDK_BRIDGE_TOKEN?.trim();
  return !token || req.headers.authorization === `Bearer ${token}`;
}

async function handle(req, res) {
  if (!authorized(req)) return json(res, 401, { error: "unauthorized" });
  if (req.method === "GET" && req.url === "/health") {
    return json(res, 200, { ok: true, sdk: "StorageSDK", providers: catalog.length });
  }
  if (req.method === "GET" && req.url === "/catalog") {
    return json(res, 200, { providers: catalog });
  }
  if (req.method !== "POST") return json(res, 404, { error: "not found" });

  const body = await readJson(req);
  const storage = await storageFor(body);
  if (req.url === "/probe") {
    const key = `.clotho/probes/${randomUUID()}`;
    const payload = `clotho-storage-sdk-probe:${key}`;
    const started = Date.now();
    await storage.upload(key, payload, { contentType: "text/plain" });
    const restored = await storage.download(key, { as: "text" });
    await storage.delete(key);
    if (restored !== payload) throw new Error("StorageSDK probe read did not match write");
    return json(res, 200, { ok: true, provider: body.provider, latency_ms: Date.now() - started });
  }
  if (req.url === "/objects/list") {
    const result = await storage.list(body.options ?? {});
    return json(res, 200, result);
  }
  if (req.url === "/snapshots/create") {
    const snapshot = await storage.snapshots.create(body.options ?? {});
    return json(res, 201, snapshot);
  }
  if (req.url === "/forks/create") {
    const fork = await storage.forks.create(body.options ?? {});
    return json(res, 201, fork);
  }
  return json(res, 404, { error: "not found" });
}

export function createServer() {
  return http.createServer((req, res) => {
    handle(req, res).catch((error) => {
      json(res, error?.status ?? 502, { error: error?.message ?? "StorageSDK bridge failure" });
    });
  });
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const port = Number(process.env.PORT ?? 8092);
  createServer().listen(port, "0.0.0.0", () => {
    console.log(`Clotho StorageSDK bridge listening on :${port}`);
  });
}

