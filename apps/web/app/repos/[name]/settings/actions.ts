"use server";

import { revalidatePath } from "next/cache";
import { redirect } from "next/navigation";

import { api } from "src/lib/api";

export async function updateRepoSettings(repo: string, formData: FormData) {
  const description = String(formData.get("description") ?? "").trim();
  const visibility = String(formData.get("visibility") ?? "").trim();
  const defaultBranch = String(formData.get("default_branch") ?? "").trim();
  const kind = String(formData.get("kind") ?? "").trim();
  const threshold = Number(formData.get("large_file_threshold_bytes"));
  const networkMode = String(formData.get("network_mode") ?? "public") as
    | "public"
    | "tailscale";
  const networkTags = String(formData.get("network_tags") ?? "")
    .split(",")
    .map((tag) => tag.trim())
    .filter(Boolean);

  await (await api()).updateRepo(repo, {
    description: description || undefined,
    visibility: visibility || undefined,
    defaultBranch: defaultBranch || undefined,
    kind: (kind || undefined) as "code" | "model" | "dataset" | undefined,
    largeFileThresholdBytes: Number.isFinite(threshold) ? threshold : undefined,
    networkMode,
    networkTags,
  });

  revalidatePath(`/repos/${repo}`);
  revalidatePath(`/repos/${repo}/settings`);
}

export async function deleteRepo(repo: string, formData: FormData) {
  const confirm = String(formData.get("confirm") ?? "").trim();
  if (confirm !== repo) {
    throw new Error(`type ${repo} to confirm deletion`);
  }
  await (await api()).deleteRepo(repo);
  revalidatePath("/");
  redirect("/");
}

export async function updateMergePolicy(repo: string, formData: FormData) {
  await (await api()).updateMergePolicy(repo, {
    require_passing_actions: formData.get("require_passing_actions") === "on",
    block_merge_when_conflicted:
      formData.get("block_merge_when_conflicted") === "on",
    require_review_approvals: Number(
      formData.get("require_review_approvals") ?? 0,
    ),
    protect_default_branch: formData.get("protect_default_branch") === "on",
  });
  revalidatePath(`/repos/${repo}/settings`);
  revalidatePath(`/repos/${repo}/pulls`);
}

export async function updateActionsPolicy(repo: string, formData: FormData) {
  const gpuTypes = String(formData.get("gpu_types") ?? "")
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
  await (await api()).updateActionsConfig(repo, {
    enabled: formData.get("enabled") === "on",
    provider: String(formData.get("provider") ?? "daytona"),
    default_image: String(formData.get("default_image") ?? ""),
    timeout_seconds: Number(formData.get("timeout_seconds") ?? 900),
    accelerator: String(formData.get("accelerator") ?? "cpu"),
    gpu_types: gpuTypes,
  });
  revalidatePath(`/repos/${repo}/settings`);
  revalidatePath(`/repos/${repo}/actions`);
}

export async function createRepoSecret(repo: string, formData: FormData) {
  const name = String(formData.get("name") ?? "").trim();
  const value = String(formData.get("value") ?? "");
  const description = String(formData.get("description") ?? "").trim();
  if (!name || !value) {
    throw new Error("name and value are required");
  }
  await (await api()).createRepoSecret(repo, { name, value, description });
  revalidatePath(`/repos/${repo}/settings`);
  redirect(`/repos/${repo}/settings#secrets`);
}

export async function deleteRepoSecret(repo: string, name: string) {
  await (await api()).deleteRepoSecret(repo, name);
  revalidatePath(`/repos/${repo}/settings`);
  redirect(`/repos/${repo}/settings#secrets`);
}

export async function importHuggingFace(repo: string, formData: FormData) {
  const repoId = String(formData.get("repo_id") ?? "").trim();
  const revision = String(formData.get("revision") ?? "main").trim() || "main";
  const paths = String(formData.get("paths") ?? "")
    .split(/[\n,]/)
    .map((path) => path.trim())
    .filter(Boolean);
  const maxFiles = Number(formData.get("max_files") ?? 200);
  const maxTotalGiB = Number(formData.get("max_total_gib") ?? 50);
  if (!repoId) throw new Error("Hugging Face namespace/name is required");
  if (!Number.isInteger(maxFiles) || maxFiles < 1 || maxFiles > 1000) {
    throw new Error("max files must be between 1 and 1000");
  }
  if (!Number.isFinite(maxTotalGiB) || maxTotalGiB < 1 || maxTotalGiB > 1024) {
    throw new Error("max import size must be between 1 and 1024 GiB");
  }
  const maxTotalBytes = Math.floor(maxTotalGiB * 1024 ** 3);
  await (await api()).startHuggingFaceImport(repo, repoId, {
    revision,
    paths,
    maxFiles,
    maxTotalBytes,
  });
  revalidatePath(`/repos/${repo}`);
  revalidatePath(`/repos/${repo}/settings`);
  redirect(`/repos/${repo}/settings#hub`);
}
