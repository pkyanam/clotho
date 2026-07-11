"use server";

import { revalidatePath } from "next/cache";
import { redirect } from "next/navigation";

import { api } from "src/lib/api";

export async function connectTailscale(formData: FormData) {
  const org = String(formData.get("org") ?? "").trim();
  const clientId = String(formData.get("client_id") ?? "").trim();
  const clientSecret = String(formData.get("client_secret") ?? "").trim();
  if (!clientId || !clientSecret) {
    throw new Error("Tailscale OAuth client id and secret are required");
  }
  await (await api()).connectProvider("tailscale", {
    org: org || undefined,
    clientId,
    clientSecret,
  });
  revalidatePath("/settings/network");
  revalidatePath("/settings/secrets");
  redirect("/settings/network");
}

export async function disconnectTailscale(formData: FormData) {
  const org = String(formData.get("org") ?? "").trim();
  await (await api()).disconnectProvider("tailscale", { org: org || undefined });
  revalidatePath("/settings/network");
  revalidatePath("/settings/secrets");
  redirect("/settings/network");
}
