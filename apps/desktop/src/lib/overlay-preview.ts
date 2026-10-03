import type { OverlayStyle, Phase } from "./bindings";

/**
 * The contract between the motion tuner (tune.html) and the overlay window.
 * Its own module because overlay.tsx mounts a React root at import time —
 * importing the event name from there would start a second overlay.
 */

/**
 * What the tuning page can ask the overlay to do. `?phase=` only sets a state
 * at load, which loses the transition into it — and that transition is the
 * motion being tuned.
 */
export type PreviewCommand = {
  phase?: Phase;
  style?: OverlayStyle;
  /**
   * The live dictation transcript. `stream` includes a mid-way revision, which
   * is the case that can make the panel jitter.
   */
  partial?: "stream" | "clear";
  /**
   * An agent run. `pending` stops on the approval gate; `contact` stops on the
   * editable one, which is otherwise only reachable by asking the agent for
   * someone it cannot find.
   */
  agent?: "run" | "pending" | "planned" | "contact" | "asked" | "choose" | "close";
  dismissing?: boolean;
  /** Rebuilds the panel's springs, which is how a changed --panel-* applies. */
  restart?: boolean;
};

/** Dispatched on the overlay window. Same origin, so nothing is serialised. */
export const PREVIEW_EVENT = "aloud-preview";
