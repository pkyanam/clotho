"use server";

import { revalidatePath } from "next/cache";

import { api } from "src/lib/api";

export async function commentOnPull(
  name: string,
  number: number,
  formData: FormData,
): Promise<void> {
  const body = String(formData.get("body") ?? "").trim();
  if (!body) throw new Error("comment body is required");
  await api().commentOnPull(name, number, body);
  revalidatePath(`/repos/${name}/pulls/${number}`);
}

export async function reviewPull(
  name: string,
  number: number,
  event: "COMMENT" | "APPROVE" | "REQUEST_CHANGES",
  formData: FormData,
): Promise<void> {
  const body = String(formData.get("body") ?? "").trim();
  await api().reviewPull(name, number, { event, body });
  revalidatePath(`/repos/${name}/pulls/${number}`);
}

export async function mergePull(
  name: string,
  number: number,
): Promise<void> {
  await api().mergePull(name, number, { method: "merge" });
  revalidatePath(`/repos/${name}/pulls/${number}`);
  revalidatePath(`/repos/${name}/pulls`);
}
