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

  await (await api()).updateRepo(repo, {
    description: description || undefined,
    visibility: visibility || undefined,
    defaultBranch: defaultBranch || undefined,
    kind: (kind || undefined) as "code" | "model" | "dataset" | undefined,
    largeFileThresholdBytes: Number.isFinite(threshold) ? threshold : undefined,
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
