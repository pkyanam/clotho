import { Badge } from "@cloudflare/kumo";
import type { FileDiff } from "@clotho/sdk-js";

import { conflictLineKind } from "src/lib/conflicts";

/**
 * The structured diff, rendered for humans: per file, the symbol-level
 * changes clotho-diff extracted (the same object agents consume through
 * diff_symbol — docs/prd.md §2) above the line hunks. Unresolved jj
 * conflicts are flagged per file and their marker lines styled, never
 * hidden (ADR-0006).
 */
export function DiffView({ files }: { files: FileDiff[] }) {
  if (files.length === 0) {
    return (
      <p className="mt-8 text-sm text-kumo-inactive">
        no changes — the trees are identical.
      </p>
    );
  }
  return (
    <div className="mt-8 space-y-8">
      {files.map((file) => (
        <section key={file.path} className="border border-kumo-hairline">
          <header className="flex flex-wrap items-center gap-3 border-b border-kumo-hairline px-4 py-2">
            <span className="text-sm">{file.path}</span>
            <Badge variant="outline">{file.status}</Badge>
            {file.language && <Badge variant="outline">{file.language}</Badge>}
            {file.conflicted && <Badge variant="outline">conflict</Badge>}
          </header>

          {file.conflicted && (
            <p className="border-b border-kumo-hairline px-4 py-2 text-xs text-kumo-subtle">
              unresolved jj conflict — landed as a first-class object, main kept
              moving. the hunks below contain jj&apos;s materialization of every
              side; resolve it with a follow-up commit.
            </p>
          )}

          {file.symbols.length > 0 && (
            <ul className="flex flex-wrap gap-x-5 gap-y-1 border-b border-kumo-hairline px-4 py-2 text-xs text-kumo-subtle">
              {file.symbols.map((symbol) => (
                <li key={`${symbol.kind}:${symbol.name}`}>
                  <span className="text-kumo-inactive">
                    {symbol.status} {symbol.kind}{" "}
                  </span>
                  {symbol.name}
                  <span className="text-kumo-inactive">
                    {symbol.new_start_line > 0
                      ? ` :${symbol.new_start_line}`
                      : symbol.old_start_line > 0
                        ? ` :${symbol.old_start_line} (old)`
                        : ""}
                  </span>
                </li>
              ))}
            </ul>
          )}

          {file.binary ? (
            <p className="px-4 py-3 text-xs text-kumo-inactive">
              binary file — no text diff.
            </p>
          ) : (
            file.hunks.map((hunk, hunkIndex) => (
              <div key={hunkIndex}>
                <div className="border-b border-kumo-hairline bg-kumo-base px-4 py-1 text-xs text-kumo-inactive">
                  @@ -{hunk.old_start},{hunk.old_lines} +{hunk.new_start},
                  {hunk.new_lines} @@
                </div>
                <pre className="overflow-x-auto text-xs leading-relaxed">
                  {hunk.lines.map((line, lineIndex) => {
                    const marker =
                      file.conflicted && conflictLineKind(line.text);
                    return (
                      <div
                        key={lineIndex}
                        className={`flex px-2 ${
                          marker
                            ? "bg-kumo-elevated font-bold"
                            : line.kind === "add"
                              ? "bg-kumo-base"
                              : ""
                        } ${line.kind === "del" ? "opacity-60" : ""}`}
                      >
                        <span className="w-9 shrink-0 select-none text-right text-kumo-inactive">
                          {line.old_line ?? ""}
                        </span>
                        <span className="w-9 shrink-0 select-none text-right text-kumo-inactive">
                          {line.new_line ?? ""}
                        </span>
                        <span className="w-6 shrink-0 select-none text-center text-kumo-inactive">
                          {line.kind === "add"
                            ? "+"
                            : line.kind === "del"
                              ? "-"
                              : ""}
                        </span>
                        <code>{line.text}</code>
                      </div>
                    );
                  })}
                </pre>
              </div>
            ))
          )}
        </section>
      ))}
    </div>
  );
}
