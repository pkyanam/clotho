/**
 * ComputeSDK upstream provider catalog for the Clotho bridge.
 *
 * Source of truth: https://docs.computesdk.com/providers.md
 * and https://docs.computesdk.com/getting-started/installation.md
 *
 * Each entry maps Clotho-stored secrets / process env (UPPER_SNAKE) to the
 * factory config for `@computesdk/<id>`. Dynamic import skips packages that
 * are not installed so the bridge still boots.
 */

/**
 * @typedef {object} ProviderSpec
 * @property {string} id
 * @property {string} name
 * @property {string} pkg
 * @property {string} factory
 * @property {string[]} required Env/secret names required to construct the provider
 * @property {string[]} [optional] Optional env names
 * @property {string} [notes]
 */

/** @type {ProviderSpec[]} */
export const PROVIDERS = [
  {
    id: "agentcore",
    name: "AWS Bedrock AgentCore",
    pkg: "@computesdk/agentcore",
    factory: "agentcore",
    // No API key — AWS default chain. Region required.
    required: ["AWS_REGION"],
    optional: [
      "AWS_ACCESS_KEY_ID",
      "AWS_SECRET_ACCESS_KEY",
      "AWS_SESSION_TOKEN",
      "AWS_PROFILE",
      "AWS_DEFAULT_REGION",
    ],
    notes: "Uses AWS credential chain; set AWS_REGION (or AWS_DEFAULT_REGION).",
  },
  {
    id: "agentuity",
    name: "Agentuity",
    pkg: "@computesdk/agentuity",
    factory: "agentuity",
    required: ["AGENTUITY_SDK_KEY"],
  },
  {
    id: "archil",
    name: "Archil",
    pkg: "@computesdk/archil",
    factory: "archil",
    required: ["ARCHIL_API_KEY", "ARCHIL_REGION"],
    optional: ["ARCHIL_DISK_ID"],
    notes: "create() may need ARCHIL_DISK_ID.",
  },
  {
    id: "beam",
    name: "Beam",
    pkg: "@computesdk/beam",
    factory: "beam",
    required: ["BEAM_TOKEN", "BEAM_WORKSPACE_ID"],
  },
  {
    id: "blaxel",
    name: "Blaxel",
    pkg: "@computesdk/blaxel",
    factory: "blaxel",
    required: ["BL_API_KEY", "BL_WORKSPACE"],
  },
  {
    id: "cloudflare",
    name: "Cloudflare",
    pkg: "@computesdk/cloudflare",
    factory: "cloudflare",
    // Runtime uses gateway worker URL + secret (setup is one-time ops).
    required: ["CLOUDFLARE_SANDBOX_URL", "CLOUDFLARE_SANDBOX_SECRET"],
    optional: ["CLOUDFLARE_API_TOKEN", "CLOUDFLARE_ACCOUNT_ID"],
    notes: "Deploy gateway once with CLOUDFLARE_API_TOKEN + ACCOUNT_ID.",
  },
  {
    id: "codesandbox",
    name: "CodeSandbox",
    pkg: "@computesdk/codesandbox",
    factory: "codesandbox",
    required: ["CSB_API_KEY"],
  },
  {
    id: "daytona",
    name: "Daytona (via ComputeSDK)",
    pkg: "@computesdk/daytona",
    factory: "daytona",
    required: ["DAYTONA_API_KEY"],
    notes: "Clotho also has a direct Rust Daytona CCI provider.",
  },
  {
    id: "declaw",
    name: "Declaw",
    pkg: "@computesdk/declaw",
    factory: "declaw",
    required: ["DECLAW_API_KEY"],
  },
  {
    id: "e2b",
    name: "E2B",
    pkg: "@computesdk/e2b",
    factory: "e2b",
    required: ["E2B_API_KEY"],
  },
  {
    id: "freestyle",
    name: "Freestyle",
    pkg: "@computesdk/freestyle",
    factory: "freestyle",
    required: ["FREESTYLE_API_KEY"],
  },
  {
    id: "hopx",
    name: "HopX",
    pkg: "@computesdk/hopx",
    factory: "hopx",
    required: ["HOPX_API_KEY"],
  },
  {
    id: "k8s",
    name: "Kubernetes",
    pkg: "@computesdk/k8s",
    factory: "k8s",
    // Prefer base64 kubeconfig for remote bridge; empty required means
    // "available when package installs and default kubeconfig works".
    required: [],
    optional: ["KUBECONFIG_B64", "KUBECONFIG", "K8S_NAMESPACE"],
    notes: "Uses kubeconfig (KUBECONFIG_B64 preferred for Clotho secrets).",
  },
  {
    id: "leap0",
    name: "Leap0",
    pkg: "@computesdk/leap0",
    factory: "leap0",
    required: ["LEAP0_API_KEY"],
  },
  {
    id: "modal",
    name: "Modal",
    pkg: "@computesdk/modal",
    factory: "modal",
    required: ["MODAL_TOKEN_ID", "MODAL_TOKEN_SECRET"],
  },
  {
    id: "namespace",
    name: "Namespace",
    pkg: "@computesdk/namespace",
    factory: "namespace",
    required: ["NSC_TOKEN"],
  },
  {
    id: "runloop",
    name: "Runloop",
    pkg: "@computesdk/runloop",
    factory: "runloop",
    required: ["RUNLOOP_API_KEY"],
  },
  {
    id: "tensorlake",
    name: "Tensorlake",
    pkg: "@computesdk/tensorlake",
    factory: "tensorlake",
    required: ["TENSORLAKE_API_KEY"],
  },
  {
    id: "upstash",
    name: "Upstash",
    pkg: "@computesdk/upstash",
    factory: "upstash",
    required: ["UPSTASH_BOX_API_KEY"],
  },
  {
    id: "vercel",
    name: "Vercel",
    pkg: "@computesdk/vercel",
    factory: "vercel",
    required: ["VERCEL_TOKEN", "VERCEL_TEAM_ID", "VERCEL_PROJECT_ID"],
  },
];

/** All secret / env names Clotho may store for ComputeSDK upstreams. */
export function allSecretNames() {
  const set = new Set();
  for (const p of PROVIDERS) {
    for (const k of p.required) set.add(k);
    for (const k of p.optional ?? []) set.add(k);
  }
  return [...set].sort();
}

/**
 * Public catalog (no secrets) for UI / health.
 */
export function catalogPublic() {
  return PROVIDERS.map((p) => ({
    id: p.id,
    name: p.name,
    pkg: p.pkg,
    required: p.required,
    optional: p.optional ?? [],
    notes: p.notes ?? "",
  }));
}

/**
 * Normalize credential bag keys to UPPER_SNAKE env style.
 * @param {Record<string, string>} creds
 */
export function normalizeCreds(creds) {
  /** @type {Record<string, string>} */
  const out = {};
  if (!creds || typeof creds !== "object") return out;
  for (const [k, v] of Object.entries(creds)) {
    if (typeof v !== "string" || !v.trim()) continue;
    // Accept both E2B_API_KEY and e2b_api_key
    const upper = k.includes("_") && k === k.toUpperCase()
      ? k
      : k
          .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
          .replace(/-/g, "_")
          .toUpperCase();
    out[upper] = v.trim();
  }
  return out;
}

/**
 * Collect process.env credentials for known secret names.
 */
export function envCredentials() {
  /** @type {Record<string, string>} */
  const out = {};
  for (const name of allSecretNames()) {
    const v = process.env[name];
    if (v && String(v).trim()) out[name] = String(v).trim();
  }
  // Alias AWS_DEFAULT_REGION → AWS_REGION when region unset.
  if (!out.AWS_REGION && out.AWS_DEFAULT_REGION) {
    out.AWS_REGION = out.AWS_DEFAULT_REGION;
  }
  return out;
}

/**
 * Whether this provider has enough credentials to attempt construction.
 * @param {ProviderSpec} spec
 * @param {Record<string, string>} creds UPPER_SNAKE
 */
export function isReady(spec, creds) {
  if (spec.id === "k8s") {
    // k8s can use default kubeconfig on the host, or explicit secret.
    return Boolean(
      creds.KUBECONFIG_B64 ||
        creds.KUBECONFIG ||
        process.env.KUBECONFIG ||
        // Allow always-try when package installed (in-cluster / default config).
        process.env.CLOTHO_COMPUTE_SDK_K8S_ALWAYS === "1",
    );
  }
  if (spec.id === "agentcore") {
    return Boolean(creds.AWS_REGION || creds.AWS_DEFAULT_REGION);
  }
  return spec.required.every((k) => creds[k] && creds[k].trim());
}

/**
 * Build factory options for a provider from UPPER_SNAKE creds.
 * @param {ProviderSpec} spec
 * @param {Record<string, string>} creds
 */
export function buildConfig(spec, creds) {
  switch (spec.id) {
    case "agentcore":
      return {
        region: creds.AWS_REGION || creds.AWS_DEFAULT_REGION,
        ...(creds.AWS_PROFILE ? { profile: creds.AWS_PROFILE } : {}),
        ...(creds.AWS_ACCESS_KEY_ID && creds.AWS_SECRET_ACCESS_KEY
          ? {
              credentials: {
                accessKeyId: creds.AWS_ACCESS_KEY_ID,
                secretAccessKey: creds.AWS_SECRET_ACCESS_KEY,
                ...(creds.AWS_SESSION_TOKEN
                  ? { sessionToken: creds.AWS_SESSION_TOKEN }
                  : {}),
              },
            }
          : {}),
      };
    case "agentuity":
      return { apiKey: creds.AGENTUITY_SDK_KEY };
    case "archil":
      return {
        apiKey: creds.ARCHIL_API_KEY,
        region: creds.ARCHIL_REGION,
        ...(creds.ARCHIL_DISK_ID ? { diskId: creds.ARCHIL_DISK_ID } : {}),
      };
    case "beam":
      return {
        token: creds.BEAM_TOKEN,
        workspaceId: creds.BEAM_WORKSPACE_ID,
      };
    case "blaxel":
      return {
        apiKey: creds.BL_API_KEY,
        workspace: creds.BL_WORKSPACE,
      };
    case "cloudflare":
      return {
        sandboxUrl: creds.CLOUDFLARE_SANDBOX_URL,
        sandboxSecret: creds.CLOUDFLARE_SANDBOX_SECRET,
      };
    case "codesandbox":
      return { apiKey: creds.CSB_API_KEY };
    case "daytona":
      return { apiKey: creds.DAYTONA_API_KEY };
    case "declaw":
      return { apiKey: creds.DECLAW_API_KEY };
    case "e2b":
      return { apiKey: creds.E2B_API_KEY };
    case "freestyle":
      return { apiKey: creds.FREESTYLE_API_KEY };
    case "hopx":
      return { apiKey: creds.HOPX_API_KEY };
    case "k8s": {
      const cfg = {};
      if (creds.KUBECONFIG_B64) {
        cfg.kubeConfigRaw = Buffer.from(creds.KUBECONFIG_B64, "base64").toString(
          "utf8",
        );
      } else if (creds.KUBECONFIG) {
        // If it looks like YAML, treat as raw; otherwise path.
        if (creds.KUBECONFIG.includes("apiVersion") || creds.KUBECONFIG.includes("clusters:")) {
          cfg.kubeConfigRaw = creds.KUBECONFIG;
        } else {
          cfg.kubeConfigPath = creds.KUBECONFIG;
        }
      }
      if (creds.K8S_NAMESPACE) cfg.namespace = creds.K8S_NAMESPACE;
      return cfg;
    }
    case "leap0":
      return { apiKey: creds.LEAP0_API_KEY };
    case "modal":
      return {
        tokenId: creds.MODAL_TOKEN_ID,
        tokenSecret: creds.MODAL_TOKEN_SECRET,
      };
    case "namespace":
      return { token: creds.NSC_TOKEN };
    case "runloop":
      return { apiKey: creds.RUNLOOP_API_KEY };
    case "tensorlake":
      return { apiKey: creds.TENSORLAKE_API_KEY };
    case "upstash":
      return { apiKey: creds.UPSTASH_BOX_API_KEY };
    case "vercel":
      return {
        token: creds.VERCEL_TOKEN,
        teamId: creds.VERCEL_TEAM_ID,
        projectId: creds.VERCEL_PROJECT_ID,
      };
    default:
      return {};
  }
}

/**
 * Instantiate all ready providers from a credential bag.
 * @param {Record<string, string>} creds
 * @returns {Promise<{ providers: any[], names: string[], skipped: {id: string, reason: string}[] }>}
 */
export async function loadProviders(creds) {
  const normalized = normalizeCreds(creds);
  const providers = [];
  const names = [];
  const skipped = [];

  for (const spec of PROVIDERS) {
    if (!isReady(spec, normalized)) {
      skipped.push({
        id: spec.id,
        reason: `missing credentials (${spec.required.join(", ") || "see notes"})`,
      });
      continue;
    }
    try {
      const mod = await import(spec.pkg);
      const factory =
        mod[spec.factory] ?? mod.default?.[spec.factory] ?? mod.default;
      if (typeof factory !== "function") {
        skipped.push({ id: spec.id, reason: `no factory ${spec.factory}` });
        continue;
      }
      const instance = factory(buildConfig(spec, normalized));
      if (instance) {
        providers.push(instance);
        names.push(instance.name ?? spec.id);
      }
    } catch (e) {
      skipped.push({
        id: spec.id,
        reason: e?.message ?? String(e),
      });
      if (process.env.CLOTHO_COMPUTE_SDK_DEBUG) {
        console.warn(`skip ${spec.id}:`, e?.message ?? e);
      }
    }
  }

  return { providers, names, skipped };
}

/**
 * Build computesdk multi-provider runtime from credentials.
 * @param {Record<string, string>} creds
 */
export async function buildRuntime(creds) {
  const { providers, names, skipped } = await loadProviders(creds);
  if (providers.length === 0) {
    return {
      runtime: null,
      error:
        "no ComputeSDK upstream providers configured (connect credentials in Clotho settings and ensure @computesdk/* packages are installed via pnpm)",
      names: [],
      skipped,
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
    return {
      runtime: { compute, names },
      error: "",
      names,
      skipped,
    };
  } catch (e) {
    return {
      runtime: null,
      error: `computesdk core unavailable: ${e?.message ?? e}`,
      names: [],
      skipped,
    };
  }
}
