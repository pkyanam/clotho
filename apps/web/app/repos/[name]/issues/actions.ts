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
  const labels = formData
    .getAll("labels")
    .map((v) => String(v).trim())
    .filter(Boolean);
  const assignee = String(formData.get("assignee") ?? "").trim();
  const assignees = assignee ? [assignee] : [];
  if (!title) throw new Error("title is required");
  const issue = await (await api()).createIssue(name, { title, body, labels, assignees });
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
  await (await api()).commentOnIssue(name, number, body);
  revalidatePath(`/repos/${name}/issues/${number}`);
}

export async function updateIssueMetadata(
  name: string,
  number: number,
  formData: FormData,
): Promise<void> {
  const labels = formData
    .getAll("labels")
    .map((v) => String(v).trim())
    .filter(Boolean);
  const assignee = String(formData.get("assignee") ?? "").trim();
  const assignees = assignee ? [assignee] : [];
  await (await api()).updateIssue(name, number, { labels, assignees });
  revalidatePath(`/repos/${name}/issues/${number}`);
  revalidatePath(`/repos/${name}/issues`);
}
