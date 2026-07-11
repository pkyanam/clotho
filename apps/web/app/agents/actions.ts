"use server";

import { revalidatePath } from "next/cache";

import { api } from "src/lib/api";

function parseScopeList(raw: string): string[] {
  const trimmed = raw.trim();
  if (!trimmed) return [];
  if (trimmed.startsWith("[")) {
    const parsed = JSON.parse(trimmed) as unknown;
    if (!Array.isArray(parsed)) {
      throw new Error("scopes must be a JSON array or comma-separated list");
    }
    return parsed.map((v) => String(v).trim()).filter(Boolean);
  }
  return trimmed
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
}

export async function createAgentAction(formData: FormData) {
  const name = String(formData.get("name") ?? "").trim();
  const description = String(formData.get("description") ?? "").trim();
  if (!name) {
    throw new Error("name is required");
  }
  await (await api()).createAgent({ name, description });
  revalidatePath("/agents");
}

export async function mintAgentTokenAction(agentName: string, formData: FormData) {
  const reposRaw = String(formData.get("allowed_repos") ?? "*");
  const toolsRaw = String(formData.get("allowed_tools") ?? "*");
  const expiresRaw = String(formData.get("expires_in_secs") ?? "").trim();
  const allowedRepos = parseScopeList(reposRaw);
  const allowedTools = parseScopeList(toolsRaw);
  if (allowedRepos.length === 0 || allowedTools.length === 0) {
    throw new Error("allowed repos and tools are required");
  }
  const expiresInSecs = expiresRaw ? Number(expiresRaw) : undefined;
  const minted = await (await api()).mintAgentToken(agentName, {
    allowedRepos,
    allowedTools,
    expiresInSecs: Number.isFinite(expiresInSecs) ? expiresInSecs : undefined,
  });
  revalidatePath("/agents");
  return minted;
}

export async function revokeAgentTokenAction(agentName: string, tokenId: string) {
  await (await api()).revokeAgentToken(agentName, tokenId);
  revalidatePath("/agents");
}

export async function updateAgentTokenScopesAction(
  agentName: string,
  tokenId: string,
  formData: FormData,
) {
  const reposRaw = String(formData.get("allowed_repos") ?? "").trim();
  const toolsRaw = String(formData.get("allowed_tools") ?? "").trim();
  const allowedRepos = reposRaw ? parseScopeList(reposRaw) : undefined;
  const allowedTools = toolsRaw ? parseScopeList(toolsRaw) : undefined;
  if (!allowedRepos && !allowedTools) {
    throw new Error("provide repos and/or tools to update");
  }
  await (await api()).updateAgentTokenScopes(agentName, tokenId, {
    allowedRepos,
    allowedTools,
  });
  revalidatePath("/agents");
}
