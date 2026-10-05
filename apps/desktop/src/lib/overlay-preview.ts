import type { CardKind, NotchStyle, OverlayStyle, Phase } from "./bindings";
import type { SectionId } from "@/features/notch/sections";
import type { TakeoverPreview } from "@/features/overlay/previews";

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
  /** The open panel's look: Settings › Overlay's choice, without saving it. */
  notchStyle?: NotchStyle;
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
   * A takeover fixture (`TAKEOVER_PREVIEWS`), routed exactly as Rust's `island`
   * event is, so the preview is the real routing. `stay` keeps it up for a
   * screenshot instead of its dwell; `waiting` is the "+N" Rust's line sends.
   */
  takeover?: TakeoverPreview;
  stay?: boolean;
  waiting?: number;
  /** Overrides the fixture's level: how a held volume key moves the bar in place. */
  level?: number;
  /**
   * The pointer over the notch: Home opens after the hover delay, and a whisper
   * with a button opens its own peek at once.
   */
  hover?: boolean;
  /** Fills Agents › Voice with past runs (`VOICE_RUN_PREVIEWS`), or empties it. */
  runs?: boolean;
  /**
   * The app icon a `card` or `toast` wears: the tuner runs in a browser, which
   * can read no app's icon, so one is handed in (a `data:` URI) to check the slot.
   */
  icon?: string;
  /**
   * Back to the resting notch at once: no run, no panel, no alert, idle. The
   * state live islands draw in, which a dictation only reaches through Rust.
   */
  rest?: boolean;
  /**
   * Live islands in the closed notch: the first `n` of `LIVE_PREVIEWS`, 0
   * for none. With no Rust behind the preview, this stands in for the
   * `island` event.
   */
  islands?: number;
  /** Opens the sections panel at a place, as a click on the notch does. */
  open?: { section: SectionId; sub?: string };
  /** Opens (or closes) ⌘K inside the open panel. */
  palette?: boolean;
  dismissing?: boolean;
  /** Rebuilds the panel's springs, which is how a changed --panel-* applies. */
  restart?: boolean;
};

/** Dispatched on the overlay window. Same origin, so nothing is serialised. */
export const PREVIEW_EVENT = "aloud-preview";
