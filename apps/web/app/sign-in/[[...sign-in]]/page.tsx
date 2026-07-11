import Link from "next/link";

function clerkConfigured(): boolean {
  return Boolean(
    process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY?.trim() ||
      process.env.CLERK_PUBLISHABLE_KEY?.trim(),
  );
}

export default async function SignInPage() {
  if (!clerkConfigured()) {
    return (
      <div className="mx-auto max-w-md px-4 py-16 text-center">
        <h1 className="text-xl text-kumo-default">sign in</h1>
        <p className="mt-3 text-sm text-kumo-inactive">
          Clerk is not configured for this deployment. Local and demo stacks use
          the bootstrap AuthProvider — set{" "}
          <code className="text-kumo-default">CLOTHO_TOKEN</code> on the web
          server, or enable Clerk with{" "}
          <code className="text-kumo-default">NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY</code>{" "}
          for managed sign-in.
        </p>
        <Link
          href="/"
          className="mt-6 inline-block text-sm text-kumo-default underline"
        >
          back to dashboard
        </Link>
      </div>
    );
  }

  const { SignIn } = await import("@clerk/nextjs");
  return (
    <div className="flex min-h-[70vh] items-center justify-center px-4">
      <SignIn
        appearance={{
          elements: {
            rootBox: "mx-auto",
            card: "border border-kumo-hairline bg-kumo-canvas shadow-none",
          },
        }}
      />
    </div>
  );
}
