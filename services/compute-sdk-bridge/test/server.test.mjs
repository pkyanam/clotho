/**
 * Unit tests for the ComputeSDK bridge without real provider packages.
 * Spawns the server with no credentials and checks health/job/catalog.
 */

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, "..");

async function withServer(fn) {
  const port = 18091 + Math.floor(Math.random() * 1000);
  const child = spawn(process.execPath, ["src/server.mjs"], {
    cwd: root,
    env: {
      ...process.env,
      PORT: String(port),
      // Ensure no accidental upstream credentials in CI.
      E2B_API_KEY: "",
      MODAL_TOKEN_ID: "",
      MODAL_TOKEN_SECRET: "",
      DAYTONA_API_KEY: "",
      VERCEL_TOKEN: "",
      CSB_API_KEY: "",
      RUNLOOP_API_KEY: "",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });

  await Promise.race([
    once(child.stdout, "data"),
    new Promise((r) => setTimeout(r, 2000)),
  ]);

  try {
    await fn(port);
  } finally {
    child.kill("SIGTERM");
    await once(child, "exit").catch(() => {});
  }
}

test("health reports unconfigured without provider credentials", async () => {
  await withServer(async (port) => {
    const res = await fetch(`http://127.0.0.1:${port}/health`);
    assert.equal(res.status, 200);
    const body = await res.json();
    assert.equal(body.configured, false);
    assert.ok(Array.isArray(body.providers));
    assert.equal(body.providers.length, 0);
    assert.ok(Array.isArray(body.catalog));
    assert.ok(body.catalog.includes("e2b"));
    assert.ok(body.catalog.includes("vercel"));
    assert.ok(body.catalog.includes("modal"));
  });
});

test("GET /catalog lists all upstreams with required fields", async () => {
  await withServer(async (port) => {
    const res = await fetch(`http://127.0.0.1:${port}/catalog`);
    assert.equal(res.status, 200);
    const body = await res.json();
    assert.ok(body.providers.length >= 20);
    const vercel = body.providers.find((p) => p.id === "vercel");
    assert.ok(vercel.required.includes("VERCEL_TOKEN"));
  });
});

test("POST /jobs fails cleanly when unconfigured", async () => {
  await withServer(async (port) => {
    const res = await fetch(`http://127.0.0.1:${port}/jobs`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ commands: ["echo hi"] }),
    });
    assert.ok(res.status >= 400);
    const body = await res.json();
    assert.ok(body.error || body.logs);
  });
});
