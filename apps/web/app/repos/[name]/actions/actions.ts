"use server";

import { revalidatePath } from "next/cache";

import { api } from "src/lib/api";

export async function runAction(repo: string, formData: FormData) {
  const workflow = String(formData.get("workflow") ?? "ci") as
    | "ci"
    | "evaluate"
    | "inference"
    | "benchmark";
  const releaseVersion = String(formData.get("release_version") ?? "").trim();
  await (await api()).createActionRun(repo, {
    actor: "web",
    workflow,
    releaseVersion,
  });
  revalidatePath(`/repos/${repo}/actions`);
}
