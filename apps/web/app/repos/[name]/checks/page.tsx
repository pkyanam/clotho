import { redirect } from "next/navigation";

export const dynamic = "force-dynamic";

export default async function ChecksPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  redirect(`/repos/${name}/actions`);
}
