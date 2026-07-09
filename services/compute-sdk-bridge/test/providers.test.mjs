/**
 * Unit tests for the full ComputeSDK provider catalog (no live packages required).
 */

import assert from "node:assert/strict";
import test from "node:test";
import {
  PROVIDERS,
  allSecretNames,
  buildConfig,
  catalogPublic,
  isReady,
  normalizeCreds,
} from "../src/providers.mjs";

test("catalog covers all documented ComputeSDK providers", () => {
  const ids = PROVIDERS.map((p) => p.id).sort();
  for (const expected of [
    "agentcore",
    "agentuity",
    "archil",
    "beam",
    "blaxel",
    "cloudflare",
    "codesandbox",
    "daytona",
    "declaw",
    "e2b",
    "freestyle",
    "hopx",
    "k8s",
    "leap0",
    "modal",
    "namespace",
    "runloop",
    "tensorlake",
    "upstash",
    "vercel",
  ]) {
    assert.ok(ids.includes(expected), `missing provider ${expected}`);
  }
  assert.equal(ids.length, 20);
});

test("public catalog exposes required secret names without values", () => {
  const cat = catalogPublic();
  const vercel = cat.find((p) => p.id === "vercel");
  assert.deepEqual(vercel.required, [
    "VERCEL_TOKEN",
    "VERCEL_TEAM_ID",
    "VERCEL_PROJECT_ID",
  ]);
  const names = allSecretNames();
  assert.ok(names.includes("E2B_API_KEY"));
  assert.ok(names.includes("MODAL_TOKEN_ID"));
  assert.ok(names.includes("UPSTASH_BOX_API_KEY"));
  assert.ok(names.includes("AGENTUITY_SDK_KEY"));
});

test("normalizeCreds uppercases camelCase and snake keys", () => {
  const n = normalizeCreds({
    e2b_api_key: "abc",
    VERCEL_TOKEN: "tok",
  });
  assert.equal(n.E2B_API_KEY, "abc");
  assert.equal(n.VERCEL_TOKEN, "tok");
});

test("isReady requires all multi-key fields for vercel and modal", () => {
  const vercel = PROVIDERS.find((p) => p.id === "vercel");
  assert.equal(isReady(vercel, { VERCEL_TOKEN: "t" }), false);
  assert.equal(
    isReady(vercel, {
      VERCEL_TOKEN: "t",
      VERCEL_TEAM_ID: "team",
      VERCEL_PROJECT_ID: "proj",
    }),
    true,
  );
  const modal = PROVIDERS.find((p) => p.id === "modal");
  assert.equal(isReady(modal, { MODAL_TOKEN_ID: "id" }), false);
  assert.equal(
    isReady(modal, { MODAL_TOKEN_ID: "id", MODAL_TOKEN_SECRET: "sec" }),
    true,
  );
});

test("buildConfig maps vercel and modal fields correctly", () => {
  assert.deepEqual(
    buildConfig(
      PROVIDERS.find((p) => p.id === "vercel"),
      {
        VERCEL_TOKEN: "t",
        VERCEL_TEAM_ID: "team",
        VERCEL_PROJECT_ID: "proj",
      },
    ),
    { token: "t", teamId: "team", projectId: "proj" },
  );
  assert.deepEqual(
    buildConfig(
      PROVIDERS.find((p) => p.id === "modal"),
      { MODAL_TOKEN_ID: "id", MODAL_TOKEN_SECRET: "sec" },
    ),
    { tokenId: "id", tokenSecret: "sec" },
  );
  assert.deepEqual(
    buildConfig(PROVIDERS.find((p) => p.id === "e2b"), { E2B_API_KEY: "k" }),
    { apiKey: "k" },
  );
});
