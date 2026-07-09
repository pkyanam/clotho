"use server";

import { revalidatePath } from "next/cache";
import { redirect } from "next/navigation";

import { api } from "src/lib/api";

export async function connectProvider(provider: string, formData: FormData) {
  const apiKey = String(formData.get("api_key") ?? "").trim();
  const org = String(formData.get("org") ?? "").trim();
  const upstream = String(formData.get("upstream") ?? "").trim();
  if (!apiKey) {
    throw new Error("api key is required");
  }
  await api().connectProvider(provider, {
    apiKey,
    org: org || undefined,
    upstream: upstream || undefined,
  });
  revalidatePath("/settings/compute");
  revalidatePath("/settings/secrets");
  revalidatePath("/");
  redirect("/settings/compute");
}

export async function disconnectProvider(
  provider: string,
  formData: FormData,
) {
  const org = String(formData.get("org") ?? "").trim();
  await api().disconnectProvider(provider, { org: org || undefined });
  revalidatePath("/settings/compute");
  revalidatePath("/settings/secrets");
  revalidatePath("/");
  redirect("/settings/compute");
}
