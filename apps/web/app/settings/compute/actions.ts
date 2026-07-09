"use server";

import { revalidatePath } from "next/cache";
import { redirect } from "next/navigation";

import { api } from "src/lib/api";

export async function connectProvider(provider: string, formData: FormData) {
  const org = String(formData.get("org") ?? "").trim();
  const upstream = String(formData.get("upstream") ?? "").trim();

  if (provider === "computesdk") {
    const credentials: Record<string, string> = {};
    for (const [key, value] of formData.entries()) {
      if (key === "org" || key === "upstream" || key === "api_key") continue;
      if (typeof value === "string" && value.trim()) {
        credentials[key] = value.trim();
      }
    }
    const apiKey = String(formData.get("api_key") ?? "").trim();
    // Single-field forms may use api_key + a hidden secret_name.
    const secretName = String(formData.get("secret_name") ?? "").trim();
    if (apiKey && secretName) {
      credentials[secretName] = apiKey;
    }
    if (Object.keys(credentials).length === 0 && !apiKey) {
      throw new Error("at least one credential is required");
    }
    await api().connectProvider(provider, {
      apiKey: apiKey || undefined,
      org: org || undefined,
      upstream: upstream || undefined,
      credentials,
    });
  } else {
    const apiKey = String(formData.get("api_key") ?? "").trim();
    if (!apiKey) {
      throw new Error("api key is required");
    }
    await api().connectProvider(provider, {
      apiKey,
      org: org || undefined,
    });
  }

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
