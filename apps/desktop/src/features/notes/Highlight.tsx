/**
 * Bolds every query term inside `text`, matching the Swift search view.
 *
 * Terms are split the same way the Rust `notes::terms` splits them, so what is
 * highlighted is exactly what scored.
 */
export function Highlight({ text, terms }: { text: string; terms: string[] }) {
  if (terms.length === 0) return <>{text}</>;

  // One pass, alternating plain and matched spans. A regex keeps the split and
  // the match in step; escaping matters because a query is arbitrary input.
  const pattern = new RegExp(`(${terms.map(escape).join("|")})`, "gi");
  const parts = text.split(pattern);

  return (
    <>
      {parts.map((part, i) =>
        // split() with a capture group puts matches at odd indices.
        // Bold, not the browser's yellow block: the classes live here rather
        // than as a `mark {}` rule so they cannot leak to the rest of the app.
        i % 2 === 1 ? (
          <mark key={i} className="bg-transparent font-bold text-inherit">
            {part}
          </mark>
        ) : (
          part
        ),
      )}
    </>
  );
}

/** A term like "c++" would otherwise be an invalid or wrong pattern. */
function escape(term: string) {
  return term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
