export default function Home() {
  return (
    <div className="mx-auto max-w-6xl px-6 py-16">
      <h1
        className="text-balance leading-[1.2]"
        style={{ fontSize: "clamp(1.5rem, 3vw, 1.875rem)" }}
      >
        app shell
      </h1>
      <p className="mt-4 max-w-xl text-sm">
        repo browser, pr view, and agent-session presence land in stage 6
        (docs/prd.md §5). this shell exists so the design system and workspace
        wiring are real from day zero.
      </p>
    </div>
  );
}
