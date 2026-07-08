"use server";

import { revalidatePath } from "next/cache";
import { redirect } from "next/navigation";

import { api } from "src/lib/api";

export async function createIssue(
  name: string,
  formData: FormData,
): Promise<void> {
  const title = String(formData.get("title") ?? "").trim();
  const body = String(formData.get("body") ?? "").trim();
  if (!title) throw new Error("title is required");
  const issue = await api().createIssue(name, { title, body });
  revalidatePath(`/repos/${name}/issues`);
  redirect(`/repos/${name}/issues/${issue.number}`);
}

export async function commentOnIssue(
  name: string,
  number: number,
  formData: FormData,
): Promise<void> {
  const body = String(formData.get("body") ?? "").trim();
  if (!body) throw new Error("comment body is required");
  await api().commentOnIssue(name, number, body);
  revalidatePath(`/repos/${name}/issues/${number}`);
}
