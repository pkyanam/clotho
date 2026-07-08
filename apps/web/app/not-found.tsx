import Link from "next/link";

export default function NotFound() {
  return (
    <div className="mx-auto max-w-6xl px-6 py-24">
      <h1 className="text-lg">not found</h1>
      <p className="mt-3 max-w-xl text-sm text-kumo-subtle">
        no such thread in the loom.
      </p>
      <p className="mt-6 text-sm">
        <Link href="/" className="underline decoration-kumo-hairline">
          back to repos
        </Link>
      </p>
    </div>
  );
}
