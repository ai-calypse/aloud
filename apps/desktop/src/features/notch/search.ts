import type { NotchAction } from "./actions";
import type { Section, SectionId } from "./sections";

/** One thing ⌘K can find. */
export type PaletteEntry = {
  /** Unique across the palette, for keys and the active descendant. */
  key: string;
  label: string;
  /** Where it is, said quietly beside it: "Agents › Coding". */
  hint: string;
  kind: "section" | "subsection" | "feature" | "action";
  /** A place to go, or an action to run. */
  to?: { section: SectionId; sub?: string };
  action?: string;
  /** More words it answers to, not shown. */
  keywords: string[];
};

/**
 * Everything ⌘K can reach: every ready section, subsection and the features
 * each holds, then every action that needs no arguments. Built from the same
 * registries the panel draws from, so search can never offer a place the
 * panel does not have — no dead UI here either.
 */
export function paletteEntries(ready: Section[], actions: NotchAction[]): PaletteEntry[] {
  const places = ready.flatMap((section) => [
    {
      key: `section:${section.id}`,
      label: section.name,
      hint: "Section",
      kind: "section" as const,
      to: { section: section.id },
      keywords: [],
    },
    ...section.subsections.flatMap((sub) => {
      const hint = `${section.name} › ${sub.name}`;
      const to = { section: section.id, sub: sub.id };
      return [
        {
          key: `sub:${section.id}/${sub.id}`,
          label: sub.name,
          hint: section.name,
          kind: "subsection" as const,
          to,
          keywords: [],
        },
        ...(sub.features ?? []).map((feature) => ({
          key: `feature:${section.id}/${sub.id}/${feature}`,
          label: feature,
          hint,
          kind: "feature" as const,
          to,
          keywords: [],
        })),
      ];
    }),
  ]);
  const runnable = actions
    .filter((a) => !a.args?.length)
    // An action that lives in a hidden section is as dead as the section.
    .filter((a) => !a.section || ready.some((s) => s.id === a.section))
    .map((a) => {
      const section = ready.find((s) => s.id === a.section);
      const sub = section?.subsections.find((s) => s.id === a.subsection);
      return {
        key: `action:${a.id}`,
        label: a.title,
        hint: section ? [section.name, sub?.name].filter(Boolean).join(" › ") : "Action",
        kind: "action" as const,
        action: a.id,
        keywords: a.keywords ?? [],
      };
    });
  return [...places, ...runnable];
}

/**
 * The entries a query finds, best first. A label that starts with the query
 * beats one where a word does, which beats the query anywhere in the label,
 * which beats a hit in the hint or keywords only. Every word of the query
 * must land somewhere. Ties keep registry order, so equals read as they do
 * in the panel. Empty finds everything.
 */
export function searchPalette(entries: PaletteEntry[], query: string, limit = 40): PaletteEntry[] {
  const words = query.toLowerCase().trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return entries.slice(0, limit);
  const q = words.join(" ");
  return entries
    .map((entry, index) => ({ entry, index, score: score(entry, q, words) }))
    .filter((hit) => hit.score > 0)
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .slice(0, limit)
    .map((hit) => hit.entry);
}

function score(entry: PaletteEntry, q: string, words: string[]): number {
  const label = entry.label.toLowerCase();
  const rest = [entry.hint, ...entry.keywords].join(" ").toLowerCase();
  if (!words.every((w) => label.includes(w) || rest.includes(w))) return 0;
  if (label.startsWith(q)) return 4;
  if (label.split(/[\s·›/-]+/).some((word) => word.startsWith(q))) return 3;
  if (label.includes(q)) return 2;
  return 1;
}

/**
 * Whether a key event came from inside the palette. The approval cards
 * listen on the window, in the capture phase, ahead of everything: without
 * this, Enter to pick a palette entry would also allow the write waiting
 * underneath it, and Escape would decline it.
 */
export function fromPalette(event: Event): boolean {
  return event.target instanceof Element && event.target.closest("[data-notch-palette]") !== null;
}
