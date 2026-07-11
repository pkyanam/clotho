import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { catalog, createServer } from "../src/server.mjs";

async function withServer(run) {
  const server = createServer();
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  try {
    await run(`http://127.0.0.1:${server.address().port}`);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
}

test("catalog exposes modular providers and agent storage primitives", () => {
  assert.deepEqual(catalog.map((p) => p.id), ["s3", "minio", "r2", "fs"]);
  assert.ok(catalog.every((p) => p.capabilities.includes("snapshots")));
  assert.ok(catalog.every((p) => p.capabilities.includes("forks")));
});

test("filesystem adapter probe writes, verifies, and cleans up", async () => {
  const root = await mkdtemp(join(tmpdir(), "clotho-storage-sdk-"));
  try {
    await withServer(async (url) => {
      const response = await fetch(`${url}/probe`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ provider: "fs", config: { root, folder: "objects" } }),
      });
      assert.equal(response.status, 200);
      const body = await response.json();
      assert.equal(body.ok, true);
      assert.equal(body.provider, "fs");
    });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

