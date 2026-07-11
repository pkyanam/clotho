"use server";

import { redirect } from "next/navigation";

import { api } from "src/lib/api";

export async function createRepo(formData: FormData): Promise<void> {
  const name = String(formData.get("name") ?? "").trim();
  const description = String(formData.get("description") ?? "").trim();
  const visibility = String(formData.get("visibility") ?? "public");
  if (!name) throw new Error("name is required");

  const repo = await (await api()).createRepo(name, { description, visibility });
  redirect(`/repos/${repo.name}`);
}
