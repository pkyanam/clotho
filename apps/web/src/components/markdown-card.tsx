import type { Components } from "react-markdown";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

function repositoryHref(repo: string, href?: string) {
  if (!href) return "#";
  if (/^(https?:|mailto:)/i.test(href) || href.startsWith("#")) return href;
  const normalized = href.replace(/^\.\//, "").replace(/^\//, "");
  if (normalized.split("/").includes("..")) return "#";
  return `/repos/${encodeURIComponent(repo)}/blob/${normalized}`;
}

function withoutFrontmatter(content: string) {
  if (!content.startsWith("---\n") && !content.startsWith("---\r\n"))
    return content;
  const match = content.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n/);
  return match ? content.slice(match[0].length) : content;
}

export function MarkdownCard({
  repo,
  content,
}: {
  repo: string;
  content: string;
}) {
  const components: Components = {
    h1: ({ children }) => (
      <h1 className="mb-4 mt-8 text-[1.5rem] leading-tight first:mt-0">
        {children}
      </h1>
    ),
    h2: ({ children }) => (
      <h2 className="mb-3 mt-8 border-b border-kumo-hairline pb-2 text-[1.125rem]">
        {children}
      </h2>
    ),
    h3: ({ children }) => (
      <h3 className="mb-2 mt-6 font-semibold">{children}</h3>
    ),
    p: ({ children }) => (
      <p className="my-3 max-w-4xl text-[0.875rem] leading-7 text-kumo-default">
        {children}
      </p>
    ),
    ul: ({ children }) => (
      <ul className="my-3 list-disc space-y-1 pl-6">{children}</ul>
    ),
    ol: ({ children }) => (
      <ol className="my-3 list-decimal space-y-1 pl-6">{children}</ol>
    ),
    blockquote: ({ children }) => (
      <blockquote className="my-4 rounded-md bg-kumo-recessed px-4 py-3 text-kumo-inactive">
        {children}
      </blockquote>
    ),
    a: ({ href, children }) => {
      const external = /^(https?:|mailto:)/i.test(href ?? "");
      return (
        <a
          href={repositoryHref(repo, href)}
          className="underline decoration-kumo-contrast/40 underline-offset-2 hover:decoration-kumo-contrast"
          {...(external
            ? { target: "_blank", rel: "nofollow noreferrer" }
            : {})}
        >
          {children}
        </a>
      );
    },
    img: ({ src, alt }) => (
      <a
        href={repositoryHref(repo, typeof src === "string" ? src : undefined)}
        className="my-3 inline-flex border border-kumo-hairline px-3 py-2 text-[0.8125rem] text-kumo-inactive hover:text-kumo-default"
        target="_blank"
        rel="nofollow noreferrer"
      >
        image: {alt || "open source"}
      </a>
    ),
    table: ({ children }) => (
      <div className="my-4 overflow-x-auto">
        <table className="w-full border-collapse text-left text-[0.8125rem]">
          {children}
        </table>
      </div>
    ),
    th: ({ children }) => (
      <th className="border border-kumo-hairline bg-kumo-base px-3 py-2 font-medium">
        {children}
      </th>
    ),
    td: ({ children }) => (
      <td className="border border-kumo-hairline px-3 py-2">{children}</td>
    ),
    pre: ({ children }) => (
      <pre className="my-4 overflow-x-auto border border-kumo-hairline bg-kumo-base p-4 text-[0.8125rem]">
        {children}
      </pre>
    ),
    code: ({ children }) => (
      <code className="bg-kumo-base px-1 py-0.5 font-mono text-[0.8125rem]">
        {children}
      </code>
    ),
  };

  return (
    <div className="border border-kumo-hairline px-5 py-4 text-[0.875rem]">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {withoutFrontmatter(content)}
      </ReactMarkdown>
    </div>
  );
}
