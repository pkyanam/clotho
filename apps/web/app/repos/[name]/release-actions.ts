"use server";

import { revalidatePath } from "next/cache";
import { redirect } from "next/navigation";

import { api } from "src/lib/api";

export async function createRelease(repo: string, formData: FormData) {
  const version = String(formData.get("version") ?? "").trim();
  if (!version) throw new Error("release version is required");
  await (await api()).createRelease(repo, version);
  revalidatePath(`/repos/${repo}`);
  redirect(`/repos/${repo}#releases`);
}
