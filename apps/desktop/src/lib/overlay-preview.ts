import type { OverlayStyle, Phase } from "./bindings";

/**
 * The contract between the motion tuner (tune.html) and the overlay window.
 *
 * Its own module rather than an export from overlay.tsx, which mounts a React
 * root at import time: anything importing the event name from there would
 * silently start a second overlay inside the page framing it.
 */

/**
 * What the tuning page can ask the overlay to do.
 *
 * Every state here is normally reached by holding a key and speaking, which is
 * not a thing you can do forty times while dragging a slider — and the `?phase=`
 * query params only set a state at load, which loses the transition into it.
 * That transition *is* the motion being tuned, so it has to be reachable
 * without a reload.
 */
export type PreviewCommand = {
  phase?: Phase;
  style?: OverlayStyle;
  /** `stream` types the answer in, so the panel can be watched chasing it. */
  ask?: "open" | "close" | "stream";
  /**
   * The live dictation transcript. `stream` includes a mid-way revision,
   * because that is what whisper actually does and it is the case that can
   * make the panel jitter — a preview that only ever grew would not show it.
   */
  partial?: "stream" | "clear";
  dismissing?: boolean;
  /** Rebuilds the panel's springs, which is how a changed --panel-* applies. */
  restart?: boolean;
};

/** Dispatched on the overlay window. Same origin, so nothing is serialised. */
export const PREVIEW_EVENT = "aloud-preview";
