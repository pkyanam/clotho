"use server";

import { revalidatePath } from "next/cache";

import { api } from "src/lib/api";

export async function runAction(repo: string) {
  await api().createActionRun(repo, { actor: "web" });
  revalidatePath(`/repos/${repo}/actions`);
}
