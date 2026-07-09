/**
 * ComputeSDK HTTP bridge for Clotho CCI (docs/adr/0013).
 *
 * Implements the minimal JSON surface clotho-compute's ComputeSdkBridgeProvider
 * expects. Uses computesdk multi-provider config when packages + credentials
 * are present; otherwise reports configured:false and rejects jobs.
 *
 * ComputeSDK docs: https://docs.computesdk.com/llms.txt
 */

import http from "node:http";

const PORT = Number(process.env.PORT || process.env.CLOTHO_COMPUTE_SDK_BRIDGE_PORT || 8091);

/** @type {{ compute: any, names: string[] } | null} */
let runtime = null;
let initError = "";

async function tryInitCompute() {
  const providers = [];
  const names = [];

  // Dynamic imports so the process starts even when optional packages are absent.
  async function tryProvider(pkg, factoryName, build) {
    try {
      const mod = await import(pkg);
      const factory = mod[factoryName] ?? mod.default?.[factoryName] ?? mod.default;
      if (typeof factory !== "function") return;
      const instance = build(factory);
      if (instance) {
        providers.push(instance);
        names.push(instance.name ?? factoryName);
      }
    } catch (e) {
      // Package not installed or factory failed — skip.
      if (process.env.CLOTHO_COMPUTE_SDK_DEBUG) {
        console.warn(`skip provider ${pkg}:`, e?.message ?? e);
      }
    }
  }

  if (process.env.E2B_API_KEY) {
    await tryProvider("@computesdk/e2b", "e2b", (e2b) =>
      e2b({ apiKey: process.env.E2B_API_KEY }),
    );
  }
  if (process.env.MODAL_TOKEN_ID && process.env.MODAL_TOKEN_SECRET) {
    await tryProvider("@computesdk/modal", "modal", (modal) =>
      modal({
        tokenId: process.env.MODAL_TOKEN_ID,
        tokenSecret: process.env.MODAL_TOKEN_SECRET,
      }),
    );
  }
  if (process.env.DAYTONA_API_KEY) {
    await tryProvider("@computesdk/daytona", "daytona", (daytona) =>
      daytona({ apiKey: process.env.DAYTONA_API_KEY }),
    );
  }

  if (providers.length === 0) {
    initError =
      "no ComputeSDK upstream providers configured (install @computesdk/* packages and set provider credentials)";
    runtime = null;
    return;
  }

  try {
    const { compute } = await import("computesdk");
    const strategy =
      process.env.CLOTHO_COMPUTE_SDK_STRATEGY === "round-robin"
        ? "round-robin"
        : "priority";
    const fallbackOnError =
      (process.env.CLOTHO_COMPUTE_SDK_FALLBACK ?? "true").toLowerCase() !==
      "false";
    compute.setConfig({
      providers,
      providerStrategy: strategy,
      fallbackOnError,
    });
    runtime = { compute, names };
    initError = "";
  } catch (e) {
    initError = `computesdk core unavailable: ${e?.message ?? e}`;
    runtime = null;
  }
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
  };
}

async function runJob(body) {
  if (!runtime) {
    const err = new Error(initError || "ComputeSDK bridge not configured");
    err.status = 503;
    throw err;
  }
  const commands = Array.isArray(body.commands) ? body.commands : [];
  if (commands.length === 0) {
    const err = new Error("at least one command is required");
    err.status = 400;
    throw err;
  }

  const timeoutMs = Math.max(1, Number(body.timeout_secs || 900)) * 1000;
  const sandbox = await runtime.compute.sandbox.create({
    timeout: timeoutMs,
    envs: body.env && typeof body.env === "object" ? body.env : {},
    ...(body.snapshot ? { template: body.snapshot } : {}),
  });

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
    provider: sandbox.provider ?? runtime.names[0] ?? "computesdk",
    sandbox_id: sandbox.sandboxId ?? sandbox.id ?? "",
  };
}

const server = http.createServer(async (req, res) => {
  try {
    const url = new URL(req.url || "/", `http://127.0.0.1:${PORT}`);
    if (req.method === "GET" && url.pathname === "/health") {
      return send(res, 200, healthBody());
    }
    if (req.method === "GET" && url.pathname === "/providers") {
      return send(res, 200, {
        providers: (runtime?.names ?? []).map((name) => ({
          id: name,
          name,
          configured: true,
        })),
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
    }),
  );
});
