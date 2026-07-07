const pillars = [
  {
    title: "jj-native engine",
    body: "built on jujutsu, git-compatible to the byte. operation log as an api, conflicts that never block, no staging area to forget.",
  },
  {
    title: "arachne storage",
    body: "xet-style chunk-level dedup. multi-gigabyte models and datasets stored once, moved fast, reconstructed exactly.",
  },
  {
    title: "agent-native interface",
    body: "agents are first-class identities — scoped credentials, checkpoints, structured diffs. not humans with a bot flag.",
  },
];

export default function Home() {
  return (
    <main className="mx-auto flex min-h-dvh max-w-4xl flex-col justify-center px-6 py-24">
      <div className="reveal">
        <span className="rounded-full border border-white/30 px-3 py-0.5 text-[11px]">
          prototype in progress
        </span>
      </div>

      <h1
        className="reveal mt-8 text-balance leading-[1.1]"
        style={{
          fontSize: "clamp(2.25rem, 6vw, 3.75rem)",
          ["--reveal-delay" as string]: "80ms",
        }}
      >
        clotho
      </h1>

      <p
        className="reveal mt-6 max-w-2xl text-lg"
        style={{ ["--reveal-delay" as string]: "160ms" }}
      >
        version control for humans and ai agents — working together, on the
        same repo, at the same time.
      </p>

      <div
        className="rule-hairline reveal mt-16"
        style={{ ["--reveal-delay" as string]: "240ms" }}
      />

      <div
        className="reveal mt-16 grid gap-4 sm:grid-cols-3"
        style={{ ["--reveal-delay" as string]: "320ms" }}
      >
        {pillars.map((pillar) => (
          <div
            key={pillar.title}
            className="edge-hover rounded-2xl border bg-surface p-7"
          >
            <h2 className="text-xl">{pillar.title}</h2>
            <p className="mt-3 text-sm">{pillar.body}</p>
          </div>
        ))}
      </div>

      <p
        className="reveal mt-16 text-xs"
        style={{ ["--reveal-delay" as string]: "400ms" }}
      >
        open source, apache-2.0. modular by design — compute, storage,
        database, and network are all swappable.
      </p>
    </main>
  );
}
