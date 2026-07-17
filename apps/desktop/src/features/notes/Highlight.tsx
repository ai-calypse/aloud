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
        i % 2 === 1 ? <mark key={i}>{part}</mark> : part,
      )}
    </>
  );
}

/** A term like "c++" would otherwise be an invalid or wrong pattern. */
function escape(term: string) {
  return term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
