/**
 * ComputeSDK HTTP bridge for Clotho CCI (docs/adr/0013, Stage 14).
 *
 * Implements the minimal JSON surface clotho-compute's ComputeSdkBridgeProvider
 * expects. Uses computesdk multi-provider config when packages + credentials
 * are present; otherwise reports configured:false and rejects jobs.
 *
 * Credentials sources (in priority for a job):
 * 1. Per-job `credentials` on POST /jobs (from Clotho secrets via gateway)
 * 2. Process environment (E2B_API_KEY, MODAL_TOKEN_*, DAYTONA_API_KEY)
 *
 * ComputeSDK docs: https://docs.computesdk.com/llms.txt
 */

import http from "node:http";

const PORT = Number(process.env.PORT || process.env.CLOTHO_COMPUTE_SDK_BRIDGE_PORT || 8091);

/** @type {{ compute: any, names: string[] } | null} */
let runtime = null;
let initError = "";

/**
 * Build a ComputeSDK multi-provider runtime from credential bags.
 * @param {Record<string, string>} creds
 */
async function buildRuntime(creds) {
  const providers = [];
  const names = [];

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
      if (process.env.CLOTHO_COMPUTE_SDK_DEBUG) {
        console.warn(`skip provider ${pkg}:`, e?.message ?? e);
      }
    }
  }

  const e2b = creds.e2b_api_key || creds.E2B_API_KEY;
  if (e2b) {
    await tryProvider("@computesdk/e2b", "e2b", (e2bFactory) =>
      e2bFactory({ apiKey: e2b }),
    );
  }
  const modalId = creds.modal_token_id || creds.MODAL_TOKEN_ID;
  const modalSecret = creds.modal_token_secret || creds.MODAL_TOKEN_SECRET;
  if (modalId && modalSecret) {
    await tryProvider("@computesdk/modal", "modal", (modal) =>
      modal({
        tokenId: modalId,
        tokenSecret: modalSecret,
      }),
    );
  }
  const daytona =
    creds.daytona_api_key || creds.DAYTONA_API_KEY || creds.api_key;
  if (daytona) {
    await tryProvider("@computesdk/daytona", "daytona", (daytonaFactory) =>
      daytonaFactory({ apiKey: daytona }),
    );
  }

  if (providers.length === 0) {
    return {
      runtime: null,
      error:
        "no ComputeSDK upstream providers configured (install @computesdk/* packages and set provider credentials)",
      names: [],
    };
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
    return { runtime: { compute, names }, error: "", names };
  } catch (e) {
    return {
      runtime: null,
      error: `computesdk core unavailable: ${e?.message ?? e}`,
      names: [],
    };
  }
}

function envCredentials() {
  return {
    e2b_api_key: process.env.E2B_API_KEY || "",
    modal_token_id: process.env.MODAL_TOKEN_ID || "",
    modal_token_secret: process.env.MODAL_TOKEN_SECRET || "",
    daytona_api_key: process.env.DAYTONA_API_KEY || "",
  };
}

async function tryInitCompute() {
  const { runtime: rt, error, names } = await buildRuntime(envCredentials());
  runtime = rt;
  initError = error;
  if (rt) {
    initError = "";
  }
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
  };
}

/**
 * Merge job credentials over env defaults for this job only.
 * @param {Record<string, string> | undefined} jobCreds
 */
async function runtimeForJob(jobCreds) {
  if (!jobCreds || typeof jobCreds !== "object") {
    return { rt: runtime, err: initError };
  }
  const merged = {
    ...envCredentials(),
    ...Object.fromEntries(
      Object.entries(jobCreds).filter(
        ([, v]) => typeof v === "string" && v.trim() !== "",
      ),
    ),
  };
  // If job brings no extra keys beyond empty strings, use process runtime.
  const hasJob =
    merged.e2b_api_key ||
    (merged.modal_token_id && merged.modal_token_secret) ||
    merged.daytona_api_key;
  if (!hasJob) {
    return { rt: runtime, err: initError };
  }
  // Prefer job-scoped runtime when credentials are supplied so Clotho secrets
  // work without restarting the sidecar.
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
  const sandbox = await rt.compute.sandbox.create({
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
