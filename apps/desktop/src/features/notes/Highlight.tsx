/** Bolds every query term inside `text`. Terms are split the way Rust's
 * `notes::terms` splits them, so what is highlighted is what scored. */
export function Highlight({ text, terms }: { text: string; terms: string[] }) {
  if (terms.length === 0) return <>{text}</>;

  // Escaping matters: a query is arbitrary user input.
  const pattern = new RegExp(`(${terms.map(escape).join("|")})`, "gi");
  const parts = text.split(pattern);

  return (
    <>
      {parts.map((part, i) =>
        // split() with a capture group puts matches at odd indices.
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
