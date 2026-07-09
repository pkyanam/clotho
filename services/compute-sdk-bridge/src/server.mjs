/**
 * ComputeSDK HTTP bridge for Clotho CCI (docs/adr/0013).
 *
 * Supports every ComputeSDK provider catalogued in providers.mjs
 * (https://docs.computesdk.com/providers.md). Credentials come from:
 * 1. Per-job `credentials` on POST /jobs (Clotho secrets via gateway)
 * 2. Process environment (dev escape hatch)
 *
 * Install packages with pnpm only (workspace member).
 */

import http from "node:http";
import {
  buildRuntime,
  catalogPublic,
  envCredentials,
  normalizeCreds,
} from "./providers.mjs";

const PORT = Number(process.env.PORT || process.env.CLOTHO_COMPUTE_SDK_BRIDGE_PORT || 8091);

/** @type {{ compute: any, names: string[] } | null} */
let runtime = null;
let initError = "";
/** @type {string[]} */
let lastSkipped = [];

async function tryInitCompute() {
  const { runtime: rt, error, names, skipped } = await buildRuntime(envCredentials());
  runtime = rt;
  initError = error;
  lastSkipped = (skipped ?? []).map((s) => `${s.id}: ${s.reason}`);
  if (rt) initError = "";
  return names;
}

function send(res, status, body) {
  const json = JSON.stringify(body);
  res.writeHead(status, {
    "content-type": "application/json",
    "content-length": Buffer.byteLength(json),
  });
  res.end(json);
}

async function readJson(req) {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  if (chunks.length === 0) return {};
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

function healthBody() {
  return {
    configured: Boolean(runtime),
    message: runtime
      ? `ComputeSDK bridge ready (${runtime.names.join(", ")})`
      : initError || "not configured",
    providers: runtime?.names ?? [],
    catalog: catalogPublic().map((p) => p.id),
  };
}

/**
 * @param {Record<string, string> | undefined} jobCreds
 */
async function runtimeForJob(jobCreds) {
  if (!jobCreds || typeof jobCreds !== "object") {
    return { rt: runtime, err: initError };
  }
  const merged = {
    ...envCredentials(),
    ...normalizeCreds(jobCreds),
  };
  const hasAny = Object.values(merged).some((v) => v && String(v).trim());
  if (!hasAny) {
    return { rt: runtime, err: initError };
  }
  const built = await buildRuntime(merged);
  return { rt: built.runtime, err: built.error };
}

async function runJob(body) {
  const { rt, err } = await runtimeForJob(body.credentials);
  if (!rt) {
    const error = new Error(err || initError || "ComputeSDK bridge not configured");
    error.status = 503;
    throw error;
  }
  const commands = Array.isArray(body.commands) ? body.commands : [];
  if (commands.length === 0) {
    const e = new Error("at least one command is required");
    e.status = 400;
    throw e;
  }

  const timeoutMs = Math.max(1, Number(body.timeout_secs || 900)) * 1000;
  const createOpts = {
    timeout: timeoutMs,
    envs: body.env && typeof body.env === "object" ? body.env : {},
    ...(body.snapshot ? { template: body.snapshot } : {}),
  };
  // Optional per-job provider override (ComputeSDK multi-provider name).
  if (body.upstream_provider || body.provider) {
    createOpts.provider = body.upstream_provider || body.provider;
  }

  const sandbox = await rt.compute.sandbox.create(createOpts);

  const logs = [];
  let exitCode = 0;
  try {
    const files = Array.isArray(body.files) ? body.files : [];
    for (const f of files) {
      if (!f?.path) continue;
      const content = Buffer.from(f.content_base64 || "", "base64").toString(
        "utf8",
      );
      if (sandbox.filesystem?.writeFile) {
        await sandbox.filesystem.writeFile(f.path, content);
      } else if (sandbox.files?.write) {
        await sandbox.files.write(f.path, content);
      }
    }

    for (const cmd of commands) {
      const result = await sandbox.runCommand(cmd);
      const stdout = result?.stdout ?? result?.output ?? "";
      const stderr = result?.stderr ?? "";
      if (stdout) logs.push(String(stdout));
      if (stderr) logs.push(String(stderr));
      exitCode = Number(result?.exitCode ?? result?.exit_code ?? 0);
      if (exitCode !== 0) break;
    }
  } finally {
    try {
      await sandbox.destroy();
    } catch {
      /* best-effort teardown */
    }
  }

  return {
    exit_code: exitCode,
    logs: logs.join(""),
    provider: sandbox.provider ?? rt.names[0] ?? "computesdk",
    sandbox_id: sandbox.sandboxId ?? sandbox.id ?? "",
  };
}

const server = http.createServer(async (req, res) => {
  try {
    const url = new URL(req.url || "/", `http://127.0.0.1:${PORT}`);
    if (req.method === "GET" && url.pathname === "/health") {
      return send(res, 200, healthBody());
    }
    if (req.method === "GET" && url.pathname === "/catalog") {
      return send(res, 200, {
        providers: catalogPublic(),
        strategy: process.env.CLOTHO_COMPUTE_SDK_STRATEGY || "priority",
      });
    }
    if (req.method === "GET" && url.pathname === "/providers") {
      return send(res, 200, {
        providers: (runtime?.names ?? []).map((name) => ({
          id: name,
          name,
          configured: true,
        })),
        catalog: catalogPublic(),
      });
    }
    if (req.method === "POST" && url.pathname === "/jobs") {
      const body = await readJson(req);
      const result = await runJob(body);
      return send(res, 200, result);
    }
    return send(res, 404, { error: "not found" });
  } catch (e) {
    const status = e?.status || 500;
    return send(res, status, {
      error: e?.message ?? String(e),
      exit_code: -1,
      logs: e?.message ?? String(e),
      provider: "computesdk",
      sandbox_id: "",
    });
  }
});

await tryInitCompute();
server.listen(PORT, "0.0.0.0", () => {
  console.log(
    JSON.stringify({
      service: "clotho-compute-sdk-bridge",
      port: PORT,
      ...healthBody(),
      skipped: lastSkipped.slice(0, 8),
    }),
  );
});
