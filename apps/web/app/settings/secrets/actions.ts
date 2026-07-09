"use server";

import { revalidatePath } from "next/cache";
import { redirect } from "next/navigation";

import { api } from "src/lib/api";

export async function createOrgSecret(org: string, formData: FormData) {
  const name = String(formData.get("name") ?? "").trim();
  const value = String(formData.get("value") ?? "");
  const description = String(formData.get("description") ?? "").trim();
  if (!name || !value) {
    throw new Error("name and value are required");
  }
  await api().createOrgSecret(org, { name, value, description });
  revalidatePath("/settings/secrets");
  revalidatePath("/settings/compute");
  redirect("/settings/secrets");
}

export async function deleteOrgSecret(org: string, name: string) {
  await api().deleteOrgSecret(org, name);
  revalidatePath("/settings/secrets");
  revalidatePath("/settings/compute");
  redirect("/settings/secrets");
}
