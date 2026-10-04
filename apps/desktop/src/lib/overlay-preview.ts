import type { CardKind, OverlayStyle, Phase } from "./bindings";

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
  agent?:
    | "run"
    | "pending"
    | "planned"
    | "contact"
    | "asked"
    | "choose"
    | "close";
  /** A run stopped on the gate of one card kind; see `CARD_PREVIEWS`. */
  card?: CardKind;
  /** A finished Finder search, as documents or as images; see `FINDER_PREVIEWS`. */
  finder?: "list" | "grid";
  /**
   * The chip a run of writes ends on. Sent with a run's panel open, it plays
   * the collapse into the chip; sent at rest, the chip growing out of the notch.
   */
  toast?: "show" | "undo" | "plain";
  /**
   * The app icon a `card` or `toast` wears: the tuner runs in a browser, which
   * can read no app's icon, so one is handed in (a `data:` URI) to check the slot.
   */
  icon?: string;
  dismissing?: boolean;
  /** Rebuilds the panel's springs, which is how a changed --panel-* applies. */
  restart?: boolean;
};

/** Dispatched on the overlay window. Same origin, so nothing is serialised. */
export const PREVIEW_EVENT = "aloud-preview";
