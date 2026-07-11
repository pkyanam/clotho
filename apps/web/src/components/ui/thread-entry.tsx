/** One comment / description card in an issue or pull request thread. */
export function ThreadEntry({
  author,
  meta,
  body,
  muted = false,
}: {
  author: string;
  meta: string;
  body: string;
  muted?: boolean;
}) {
  return (
    <article className="border border-kumo-hairline bg-kumo-base">
      <div className="flex flex-wrap items-baseline gap-2 border-b border-kumo-hairline px-4 py-2.5 text-[0.8125rem]">
        <span className="text-kumo-default">{author}</span>
        <span className="text-kumo-inactive">{meta}</span>
      </div>
      <p
        className={`whitespace-pre-wrap px-4 py-3.5 text-[0.9375rem] leading-relaxed ${
          muted ? "text-kumo-inactive" : "text-kumo-default"
        }`}
      >
        {body}
      </p>
    </article>
  );
}
