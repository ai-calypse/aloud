import type { Nav } from "./sections";

/**
 * How long the pointer must rest on the resting notch before it opens Home.
 * Long enough that a cursor sweeping to the menu bar's clock never opens it, short
 * enough that a deliberate hover does not feel like waiting. The old agents
 * row's 500ms card dwell was tuned for reading one card, not for opening a
 * panel, so it is not reused.
 */
export const HOVER_INTENT_MS = 400;

export type Shell = "closed" | "peek" | "open";

/**
 * Whether the shell is on screen, and why.
 *
 * - `open`: the user asked for it — a click, a typed key, a voice command, an
 *   approval. It stays until closed, whatever the pointer does.
 * - `peek`: the pointer has rested on the notch (`dwelled`). It is Home, and it
 *   is only there while the pointer is: leaving closes it.
 * - A whisper that has buttons already opens its own peek on hover, and that is
 *   the one the user is pointing at, so it wins; Home opens once the whisper
 *   has gone and the pointer is still there.
 * - Nothing opens over a dictation, a run or a toast (`resting` is false).
 */
export function shellState(s: {
  browsing: boolean;
  resting: boolean;
  hovering: boolean;
  dwelled: boolean;
  whisperOpen: boolean;
}): Shell {
  if (s.browsing) return "open";
  return s.resting && s.hovering && s.dwelled && !s.whisperOpen ? "peek" : "closed";
}

/**
 * Home, whichever section the user last chose. A hover is a glance at what is
 * live, so it is not where they left off, and it does not move where they did.
 */
export function homeNav(nav: Nav): Nav {
  return { section: "now", subs: { ...nav.subs, now: "overview" } };
}
