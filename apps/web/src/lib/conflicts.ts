/**
 * Unresolved conflicts are materialized as marker text (ADR-0006): a
 * `<<<<<<<`/`>>>>>>>` envelope with `%%%%%%%` (diff from base) and
 * `+++++++`/`-------` (side contents) section markers. The gateway flags
 * conflicted files; these helpers let the UI style the marker lines rather
 * than dumping raw text on the reviewer.
 */

export type ConflictLineKind = "open" | "close" | "section" | null;

export function conflictLineKind(line: string): ConflictLineKind {
  if (line.startsWith("<<<<<<<")) return "open";
  if (line.startsWith(">>>>>>>")) return "close";
  if (
    line.startsWith("%%%%%%%") ||
    line.startsWith("+++++++") ||
    line.startsWith("-------")
  )
    return "section";
  return null;
}
