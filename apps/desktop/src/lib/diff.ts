/**
 * A line diff for the permission card: what an Edit is about to do, readable
 * at a glance the way the terminal shows it.
 *
 * ponytail: common prefix/suffix trim, not LCS. An edit's old and new text
 * overwhelmingly share their head and tail — trim both, mark what is left as
 * removed/added, keep a couple of context lines. Interleaved changes render
 * as one del/add block instead of several; Myers is the upgrade path if that
 * ever reads poorly in practice.
 */

export type DiffLine = {
  kind: "same" | "add" | "del";
  text: string;
};

/** Context lines kept on each side of the change. */
const CONTEXT = 2;

export function diffLines(before: string, after: string): DiffLine[] {
  const a = before.length ? before.split("\n") : [];
  const b = after.length ? after.split("\n") : [];

  let prefix = 0;
  while (prefix < a.length && prefix < b.length && a[prefix] === b[prefix]) {
    prefix += 1;
  }
  let suffix = 0;
  while (
    suffix < a.length - prefix &&
    suffix < b.length - prefix &&
    a[a.length - 1 - suffix] === b[b.length - 1 - suffix]
  ) {
    suffix += 1;
  }

  const out: DiffLine[] = [];
  for (const line of a.slice(Math.max(0, prefix - CONTEXT), prefix)) {
    out.push({ kind: "same", text: line });
  }
  for (const line of a.slice(prefix, a.length - suffix)) {
    out.push({ kind: "del", text: line });
  }
  for (const line of b.slice(prefix, b.length - suffix)) {
    out.push({ kind: "add", text: line });
  }
  for (const line of a.slice(a.length - suffix, a.length - suffix + CONTEXT)) {
    out.push({ kind: "same", text: line });
  }
  return out;
}

/** `+N −M`, for the card's header chip. */
export function diffStat(lines: DiffLine[]): string {
  const added = lines.filter((l) => l.kind === "add").length;
  const removed = lines.filter((l) => l.kind === "del").length;
  const parts = [];
  if (added) parts.push(`+${added}`);
  if (removed) parts.push(`−${removed}`);
  return parts.join(" ") || "no change";
}
