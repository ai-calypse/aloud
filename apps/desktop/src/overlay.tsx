import React, {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import ReactDOM from "react-dom/client";
import { CheckIcon, CopyIcon, LoaderCircleIcon, XIcon } from "lucide-react";
import type { ToolUIPart } from "ai";
import { commands, events, type OverlayStyle, type Phase } from "./lib/bindings";
import { MessageResponse } from "@/components/ai-elements/message";
import { Shimmer } from "@/components/ai-elements/shimmer";
import {
  Tool,
  ToolContent,
  ToolHeader,
  ToolInput,
  ToolOutput,
} from "@/components/ai-elements/tool";
import {
  Confirmation,
  ConfirmationAction,
  ConfirmationActions,
  ConfirmationRequest,
  ConfirmationTitle,
} from "@/components/ai-elements/confirmation";
import {
  pendingStep,
  withCall,
  withResult,
  type Run,
  type Step,
} from "./lib/agent-steps";
import { bodyFor, labelFor } from "./lib/tool-views";
import { notchPath } from "./lib/notch-shape";
import { PREVIEW_EVENT, type PreviewCommand } from "./lib/overlay-preview";
import { createSpring } from "./lib/spring";
// Tailwind, which this window used to do without. It earns its place now that
// the Ask answer streams into this panel: Streamdown styles every element it
// emits with utilities (see the `@source` note in index.css), and hand-rolling
// a second markdown stylesheet here would be the same answer looking different
// in this window and in the agent panel.
import "./index.css";
import "./styles/overlay.css";

const LABEL: Record<Phase, string> = {
  idle: "",
  recording: "Listening…",
  transcribing: "Transcribing…",
  done: "Pasted",
  noSpeech: "No speech detected",
  // Replaced by the real reason as soon as Rust sends one; this is only what
  // shows if an error somehow arrives without a message.
  error: "Failed",
};

/** A long error still has to fit one line on a small pill. */
const ERROR_CHARS = 90;

/**
 * Bars in the listening wave.
 *
 * Five, as the Swift app used. The count and the per-bar phase step together
 * set how much of a wave fits across the row (see the delay in overlay.css) —
 * change this and that has to move with it.
 */
const BAR_COUNT = 5;

/**
 * Per-bar height ceiling: a raised cosine, tallest in the middle.
 *
 * Without it every bar reaches the same height and the row reads as a ribbon
 * rather than a waveform. Computed rather than hand-tuned so the shape holds
 * if BAR_COUNT changes.
 *
 * The floor is 0.6, not 0, because this multiplies through the wave's trough
 * and the silent-level scale as well: a shallower envelope crushed the outer
 * bars to a 1px dash at rest, which read as a broken row of dots rather than
 * as short bars.
 */
const BAR_PEAKS = Array.from({ length: BAR_COUNT }, (_, i) => {
  const position = i / (BAR_COUNT - 1); // 0…1 across the row
  return 0.6 + 0.4 * Math.sin(Math.PI * position);
});

/**
 * The drop-down panel's motion, ported from the reference implementation's
 * `balanced` preset — now declared in motion.css, alongside every other motion
 * value in the app, and read from there.
 *
 * The fallbacks are only for a stylesheet that has not applied yet; the numbers
 * that matter, and the note explaining them, live in `motion.css`.
 */
const PANEL_FALLBACK = { damping: 0.77, open: 0.45, close: 0.55 };

function readPanelSpring(element: Element) {
  const computed = getComputedStyle(element);
  const read = (name: string, fallback: number) => {
    const value = parseFloat(computed.getPropertyValue(name));
    return Number.isFinite(value) ? value : fallback;
  };
  return {
    damping: read("--panel-damping", PANEL_FALLBACK.damping),
    open: read("--panel-open", PANEL_FALLBACK.open),
    close: read("--panel-close", PANEL_FALLBACK.close),
  };
}

/**
 * How the corner radii open up as the panel grows past the cutout.
 *
 * At rest the panel *is* the notch, so it wears the notch's own corners
 * (`baseRadius = height / 3`, and the top pair 4pt tighter — the reference's
 * collapsed values, ported directly). As it grows the corners open with it,
 * which is the same thing the reference does by handing its expanded states a
 * much larger fixed pair. Derived from the *current* height rather than the
 * target, so the radii interpolate along with the box instead of jumping to
 * their final value on the first frame.
 */
const RADIUS = { topGrowth: 0.12, topMax: 16, bottomGrowth: 0.14, bottomMax: 22 };

/** Only for the `?style=pill` browser preview, where no notch is measured. */
const NOTCH_FALLBACK = { width: 200, height: 32 };

/**
 * How much wider than the cutout the panel settles, at minimum.
 *
 * The floor exists so the panel is never narrower than the hole it hangs from;
 * this is how far past it we deliberately go. Content wider than this still
 * wins, so a long error message grows the panel rather than being squeezed.
 */
const PANEL_WIDTH_FACTOR = 1.5;

/**
 * Grows the drop-down panel out of the notch and shrinks it back in.
 *
 * This is the one surface that does not use a CSS transition, for the reason
 * `lib/spring.ts` exists: its size is retargeted mid-flight (a phase change
 * lands while the previous one is still settling, three times a dictation), and
 * a transition restarts its curve from zero velocity every time it is
 * redirected. The reference gets this free from SwiftUI, whose springs are
 * interruptible by construction.
 *
 * Per frame it writes the panel's box and the shape's `d` straight onto the
 * nodes. No React state is involved — 60 renders a second is exactly what the
 * app's motion rule forbids, and the subtree would rebuild under the animation.
 */
function useNotchPanel(
  style: OverlayStyle,
  run: number,
  dismissing: boolean,
  /** Called with the box the panel is settling to, whenever that changes. */
  onTarget: (width: number, height: number) => void,
) {
  const surface = useRef<HTMLDivElement>(null);
  const shape = useRef<SVGSVGElement>(null);
  const path = useRef<SVGPathElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const driver = useRef<{
    open: () => void;
    collapse: () => void;
    remeasure: () => void;
  } | null>(null);

  // Rebuilt each time the panel appears, so every dictation starts its growth
  // from the cutout rather than from wherever the last one left the springs.
  useEffect(() => {
    const surfaceElement = surface.current;
    const shapeElement = shape.current;
    const pathElement = path.current;
    const contentElement = content.current;
    if (!surfaceElement || !shapeElement || !pathElement || !contentElement) return;
    // Nothing is on screen yet (the pill is `display: none` until the first
    // run), so there is no layout to measure and no growth to start. Measuring
    // here would set the target from a zero-width box.
    if (!contentElement.offsetWidth) return;

    // Read once, here, rather than per retarget: this is a style recalc, and
    // `remeasure` fires once a frame while an answer streams. Re-read on every
    // rebuild, so a `run` bump is what applies a change from the tuning page.
    const PANEL = readPanelSpring(surfaceElement);

    let base = { width: NOTCH_FALLBACK.width, height: NOTCH_FALLBACK.height };
    let target = base;
    let baseRadius = NOTCH_FALLBACK.height / 3;
    let collapsed = false;

    // getComputedStyle forces a style recalc, so it is called when the layout
    // changes and never from inside a frame.
    const measure = () => {
      const computed = getComputedStyle(surfaceElement);
      // A screen with no notch reports a real zero, which is different from the
      // variable being absent (the browser preview, where there is no Rust to
      // measure anything). `||` conflates the two and would invent a cutout on
      // an iMac — the panel then reserves space to clear a notch that is not
      // there and carries a strip of dead black. Only a non-number falls back.
      const measured = (name: string, fallback: number) => {
        const value = parseFloat(computed.getPropertyValue(name));
        return Number.isFinite(value) ? value : fallback;
      };
      const notchWidth = measured("--notch-w", NOTCH_FALLBACK.width);
      const notchHeight = measured("--notch-h", NOTCH_FALLBACK.height);

      // The reference's `baseRadius`: the cutout's own rounding, which every
      // other radius here is measured from.
      baseRadius = notchHeight / 3;
      base = { width: notchWidth, height: notchHeight };
      const settledHeight = notchHeight + contentElement.offsetHeight;
      target = {
        // The body sits between the two concave shoulders, so the panel has to
        // be wider than its content by exactly that much. Uses the *settled*
        // top radius, since that is the width the panel comes to rest at.
        //
        // Never narrower than the cutout: a short status word ("Pasted") wants
        // less width than the notch, and a panel narrower than the hole it
        // hangs from leaves the physical camera bump sticking out either side.
        // Every content size in the reference is likewise `baseWidth + n`.
        width: Math.max(
          notchWidth * PANEL_WIDTH_FACTOR,
          contentElement.offsetWidth + topRadiusAt(settledHeight) * 2,
        ),
        height: settledHeight,
      };
      onTarget(target.width, target.height);
    };

    const topRadiusAt = (height: number) =>
      Math.min(
        RADIUS.topMax,
        Math.max(0, baseRadius - 4) + Math.max(0, height - base.height) * RADIUS.topGrowth,
      );

    const size = { width: 0, height: 0 };
    const draw = () => {
      // Floored at the cutout, in both dimensions, because the spring rings
      // past its target and a close targets the cutout itself.
      //
      // The overshoot is the point — it is what makes the panel read as thrown
      // out of the notch rather than resized — but on the way back in it is an
      // undershoot, and a panel *smaller* than the hole it hangs from shows the
      // physical camera bump either side of it. At the damping this runs, the
      // ring is a fifth of the travel: a 332pt answer collapsed clean through
      // the 32pt cutout to nothing and sprang back out before settling. The
      // spring keeps its real state and keeps integrating; only what is drawn
      // is clamped, so nothing about the motion's timing changes.
      const width = Math.max(base.width, size.width);
      const height = Math.max(base.height, size.height);

      const grown = height - base.height;
      const bottomRadius = Math.min(
        RADIUS.bottomMax,
        baseRadius + grown * RADIUS.bottomGrowth,
      );

      surfaceElement.style.width = `${width}px`;
      surfaceElement.style.height = `${height}px`;
      shapeElement.setAttribute("viewBox", `0 0 ${width} ${height}`);
      pathElement.setAttribute(
        "d",
        notchPath(width, height, topRadiusAt(height), bottomRadius),
      );
    };

    const width = createSpring(
      (value) => {
        size.width = value;
        draw();
      },
      { damping: PANEL.damping, response: PANEL.open },
    );
    const height = createSpring(
      (value) => {
        size.height = value;
        draw();
      },
      { damping: PANEL.damping, response: PANEL.open },
    );

    measure();
    size.width = base.width;
    size.height = base.height;
    width.jump(base.width);
    height.jump(base.height);
    width.to(target.width, PANEL.open);
    height.to(target.height, PANEL.open);

    driver.current = {
      open() {
        collapsed = false;
        measure();
        width.to(target.width, PANEL.open);
        height.to(target.height, PANEL.open);
      },
      collapse() {
        collapsed = true;
        width.to(base.width, PANEL.close);
        height.to(base.height, PANEL.close);
      },
      remeasure() {
        measure();
        // A label that changes while the panel is collapsing must not drag it
        // back open — the window is already on its way out.
        if (collapsed) return;
        width.to(target.width, PANEL.open);
        height.to(target.height, PANEL.open);
      },
    };

    const observer = new ResizeObserver(() => driver.current?.remeasure());
    observer.observe(contentElement);

    return () => {
      observer.disconnect();
      width.stop();
      height.stop();
      driver.current = null;
    };
  }, [style, run]);

  // Declared after the effect above so that on a rebuild the driver exists
  // before this runs — effects fire in declaration order.
  useEffect(() => {
    if (dismissing) driver.current?.collapse();
    else driver.current?.open();
  }, [dismissing]);

  return { surface, shape, path, content };
}

/**
 * A question and the answer streaming in under it, or null when the panel is
 * just a status pill.
 *
 * There is no separate Ask window any more: the panel that has been saying
 * "Listening…" grows into the answer, so the two read as one thing rather than
 * as a second surface arriving over the first.
 */
type Ask = {
  question: string;
  answer: string;
  /** Set instead of an answer when the request failed. */
  error: string | null;
  /** The stream has finished — stops the follow-to-bottom and re-arms input. */
  done: boolean;
};

/**
 * Dev/QA: `?ask` reaches the answer state without speaking into a mic, and
 * `?ask=<ms>` arrives that late — the only way to watch the panel grow from a
 * status word into an answer, which is the transition the whole change is for.
 */
const ASK_PREVIEW: Ask = {
  question: "What is the difference between a spring and an easing curve?",
  answer:
    "An **easing curve** is a shape someone drew; a **spring** is a shape physics produced.\n\n- a bezier decelerates on a schedule\n- a spring *settles*, and can be redirected mid-flight without restarting\n\nThat second property is why retargetable motion has to integrate a real oscillator rather than replay a baked curve.",
  error: null,
  done: true,
};

/** How fast `ask: "stream"` types, in ms between chunks. */
const STREAM_MS = 45;

/**
 * A dictation preview, as whisper actually produces one.
 *
 * Each entry is the *whole* transcript at that moment, not a delta — that is
 * the shape of the real event, and it is why the text can go backwards. The
 * two shortenings here are deliberate: a rolling re-read revises what it
 * thought it heard, and a panel that springs to fit every revision is the one
 * way this can look nervous. A preview that only ever grew would hide that.
 */
const PARTIAL_PREVIEW = [
  "So the",
  "So the thing about",
  "So the thing about springs is",
  // Revised: "springs is" became "spring is", and the tail was re-heard.
  "So the thing about a spring is that",
  "So the thing about a spring is that you can redirect it",
  "So the thing about a spring is that you can redirect it mid flight and it",
  // Revised shorter again, the case that actually shrinks the panel.
  "So the thing about a spring is that you can redirect it mid-flight",
  "So the thing about a spring is that you can redirect it mid-flight and it keeps the velocity it already had, which is the whole reason",
  "So the thing about a spring is that you can redirect it mid-flight and it keeps the velocity it already had, which is the whole reason it does not stall halfway.",
];

/** How fast `partial: "stream"` advances, in ms between revisions. */
const PARTIAL_MS = 320;

/** A canned agent run for the tuning page; see PreviewCommand. */
const AGENT_PREVIEW: Run = {
  heard: "Send Priya the release notes and draft a reply to Sam",
  steps: [
    {
      name: "GMAIL_SEND_EMAIL",
      app: "Gmail",
      args: { recipient_email: "priya@example.com", subject: "Release notes" },
      requiresConfirmation: true,
      confirmed: true,
      result: { success: true, message: "Sent" },
    },
    {
      name: "GMAIL_CREATE_EMAIL_DRAFT",
      app: "Gmail",
      args: { recipient_email: "sam@example.com", subject: "Re: Thursday" },
      requiresConfirmation: true,
    },
  ],
};

/**
 * The recording pill. Rust drives the window's visibility, so this only ever
 * renders the current phase.
 */
function Overlay() {
  // Dev/QA: `?phase=recording` renders the pill statically in a browser so it
  // can be seen and screenshotted without holding the dictate hotkey. In the
  // real app there is no query string, so this is idle until Rust drives it.
  const preview = new URLSearchParams(location.search).get("phase") as Phase | null;
  const [phase, setPhase] = useState<Phase>(preview ?? "idle");
  // Rust picks the style and pushes it on every show — including the swap back
  // to the pill for an error, which no wing is wide enough to show.
  // Only what is drawn before Rust's first style event lands, so it matches the
  // default in `Settings` rather than being a second opinion about it.
  const [style, setStyle] = useState<OverlayStyle>(
    (new URLSearchParams(location.search).get("style") as OverlayStyle) ??
      "dropPill",
  );
  const [error, setError] = useState<string | null>(null);
  const [download, setDownload] = useState<number | null>(null);
  // The words so far, while the live preview is on. Feedback only — none of
  // this is ever pasted; it is a rolling re-read of the last few seconds by
  // the small local model, so it revises itself as a sentence lands.
  const [partial, setPartial] = useState("");
  // Latched once a run has produced words; see `effectiveStyle`.
  const [droppedDown, setDroppedDown] = useState(false);
  const [ask, setAsk] = useState<Ask | null>(null);
  const [agent, setAgent] = useState<Run | null>(null);
  // Either of them owns the panel; almost everything below cares only that one
  // of them does, not which.
  const panelOpen = ask !== null || agent !== null;
  // The phase listener subscribes once and has to know whether a panel is on
  // screen. A ref rather than the state it mirrors: re-subscribing on every
  // streamed token would tear down and rebuild the listener 50 times an answer.
  const openRef = useRef(panelOpen);
  openRef.current = panelOpen;
  // Rust tells us the window is about to go away, so the panel can collapse
  // back into the notch while there is still a surface to draw it on.
  const [dismissing, setDismissing] = useState(false);
  // Bumped once per dictation. The panel keys its springs off this so each run
  // starts from the cutout; the phase alone cannot say "a new run began",
  // because a run no longer ends by returning to `idle`.
  const [run, setRun] = useState(0);
  // Dev/QA, alongside `?phase=`: `?dismiss=800` plays the exit 800ms in. The
  // exit cannot be reached any other way outside the app — it is driven by an
  // event Rust emits — and it is the one path here that races (a dictation
  // starting inside the exit window), so it needs to be reachable to look at.
  const dismissAfter = Number(new URLSearchParams(location.search).get("dismiss"));
  useEffect(() => {
    if (!dismissAfter) return;
    const timer = setTimeout(() => setDismissing(true), dismissAfter);
    return () => clearTimeout(timer);
  }, [dismissAfter]);
  // The answer's own hatch; see ASK_PREVIEW.
  const askAfter = new URLSearchParams(location.search).get("ask");
  useEffect(() => {
    if (askAfter === null) return;
    const timer = setTimeout(() => setAsk(ASK_PREVIEW), Number(askAfter) || 0);
    return () => clearTimeout(timer);
  }, [askAfter]);
  // Rust decides when this window may take clicks by testing the pointer
  // against the panel's box (see `track_cursor` in overlay.rs), so it needs the
  // box. Reported from the one place that already computes it, and only when
  // it changes: the target moves on every remeasure, which while an answer is
  // streaming is roughly once a frame.
  const transcript = !panelOpen && phase === "recording" && partial.trim().length > 0;

  /**
   * The style actually drawn, which is not always the one that was chosen.
   *
   * A wing is a 40pt tab inside the menu bar and has nowhere to put a sentence,
   * so the moment there are words it hands over to the drop-down panel. Both
   * are welded to the same cutout, so what the user sees is the notch itself
   * pulling down — not one overlay being swapped for another.
   *
   * `droppedDown` latches for the rest of the run rather than tracking the
   * words: the transcript is gone by `transcribing`, and springing back up to a
   * menu-bar tab at that moment would undo the drop they just watched, one beat
   * before the panel has anything to say about the result.
   */
  const effectiveStyle: OverlayStyle =
    style === "wings" && (droppedDown || panelOpen) ? "dropPill" : style;

  const reported = useRef("");
  const reportBox = useCallback((width: number, height: number) => {
    const box = `${Math.round(width)}x${Math.round(height)}`;
    if (box === reported.current) return;
    reported.current = box;
    // Absent in the browser preview, where `invoke` throws synchronously. This
    // is measured from inside the panel's own layout pass, so letting that
    // escape would take the whole overlay down rather than just the reporting.
    try {
      void commands.setPanelBox(width, height);
    } catch {
      /* no Tauri; nothing is polling the pointer either */
    }
  }, []);
  const panel = useNotchPanel(effectiveStyle, run, dismissing, reportBox);
  // The root, not the bar row: --level inherits, so the wings style can draw
  // two waves off one write.
  const root = useRef<HTMLDivElement>(null);
  const answerRef = useRef<HTMLDivElement>(null);
  const transcriptRef = useRef<HTMLParagraphElement>(null);

  // Levels land every 50ms. Writing --level straight onto the node keeps them
  // out of React entirely: at 20 renders a second the pill would rebuild its
  // subtree while the compositor is mid-wave, which is exactly the stutter the
  // CSS-owned animation exists to avoid. Null (no signal yet) reads as silence.
  useEffect(() => {
    const unlisten = events.audioLevel.listen((e) => {
      const level = Math.min(1, Math.max(0, e.payload ?? 0));
      root.current?.style.setProperty("--level", String(level));
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  useEffect(() => {
    const unlistenPhase = events.dictationPhase.listen((e) => {
      // `idle` means the run is over, not that the pill should vanish: the
      // window stays up for the length of the exit, and something has to be on
      // it to animate. Hold the last real phase and start collapsing instead.
      //
      // This is what made a no-speech run cut off dead while a pasted one
      // animated. Pasting ends on `done`, which lingers and then dismisses with
      // the phase unchanged; silence, a spoken ask and a cancel all end on
      // `idle`, which used to blank the surface in the same frame the collapse
      // was asked to start.
      if (e.payload === "idle") {
        // …unless an answer is on screen. Both Ask paths end the run on
        // `idle`, and the run ending is exactly when the answer is finished
        // and wants reading. Rust suppresses the matching window hide for the
        // same reason (ANSWERING in overlay.rs); this is its other half.
        if (!openRef.current) setDismissing(true);
        return;
      }

      setPhase(e.payload);
      // A phase arriving means the window is staying up, which cancels an exit
      // in flight — Rust has already bumped its own generation to match.
      setDismissing(false);
      // A new run clears the last run’s message.
      if (e.payload === "recording") {
        setError(null);
        // A dictation started over the top of an answer takes the panel back;
        // `overlay::show` has already cleared its side of the same state.
        setAsk(null);
        setAgent(null);
        // Rust no longer sends an empty partial to clear the last run: there is
        // no separate window to show, so there is no show to hang it off.
        setPartial("");
        setDroppedDown(false);
        // Rebuilds the panel so it grows out of the notch again rather than
        // resuming from wherever the last run's springs were left.
        setRun((previous) => previous + 1);
      }
    });
    const unlistenDismiss = events.overlayDismiss.listen(() => setDismissing(true));
    const unlistenError = events.overlayError.listen((e) => setError(e.payload));
    const unlistenStyle = events.overlayStyle.listen((e) => {
      setStyle(e.payload.style);
      // The exact notch cutout, measured natively, so the wings weld to it
      // rather than straddle a fixed guess. Written as vars the CSS reads;
      // zero for a pill, where the wave falls back to its own gap/height.
      root.current?.style.setProperty("--notch-w", `${e.payload.notchWidth ?? 0}px`);
      root.current?.style.setProperty(
        "--notch-h",
        `${e.payload.notchHeight ?? 0}px`,
      );
    });
    // First use of the local engine fetches ~150MB; without this the pill sits
    // on "Transcribing…" for minutes and the app looks hung.
    const unlistenDownload = events.modelDownload.listen((e) =>
      setDownload(e.payload === null || e.payload >= 1 ? null : e.payload),
    );
    // The live transcript, which used to have a window of its own below the
    // pill. Rust sends an empty string to say the preview is over.
    const unlistenPartial = events.dictationPartial.listen((e) => {
      setPartial(e.payload);
      // One-way for the rest of the run: see `effectiveStyle`.
      if (e.payload.trim()) setDroppedDown(true);
    });
    return () => {
      unlistenPhase.then((off) => off());
      unlistenDismiss.then((off) => off());
      unlistenError.then((off) => off());
      unlistenStyle.then((off) => off());
      unlistenDownload.then((off) => off());
      unlistenPartial.then((off) => off());
    };
  }, []);

  // The answer stream. Separate from the phase listener above because it is
  // not a phase: the run is over by the time the last token lands.
  useEffect(() => {
    const unlisten = events.ask.listen(({ payload }) => {
      switch (payload.kind) {
        case "question":
          // A follow-up reuses the panel, so a new question resets it.
          setAsk({ question: payload.value, answer: "", error: null, done: false });
          break;
        case "delta":
          setAsk((a) => a && { ...a, answer: a.answer + payload.value });
          break;
        case "done":
          setAsk((a) => a && { ...a, done: true });
          break;
        case "failed":
          setAsk((a) => a && { ...a, error: payload.value, done: true });
          break;
      }
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  // The tuning page's hatch; see PreviewCommand. Inert in the real app, which
  // never dispatches it.
  useEffect(() => {
    let typing = 0;
    const stream = () => {
      clearInterval(typing);
      const chunks = ASK_PREVIEW.answer.match(/\S+\s*/g) ?? [];
      let at = 0;
      setAsk({ ...ASK_PREVIEW, answer: "", done: false });
      typing = window.setInterval(() => {
        if (at >= chunks.length) {
          clearInterval(typing);
          setAsk((a) => a && { ...a, done: true });
          return;
        }
        const chunk = chunks[at++];
        setAsk((a) => a && { ...a, answer: a.answer + chunk });
      }, STREAM_MS);
    };

    const dictate = () => {
      clearInterval(typing);
      let at = 0;
      setPartial("");
      typing = window.setInterval(() => {
        if (at >= PARTIAL_PREVIEW.length) {
          clearInterval(typing);
          return;
        }
        setPartial(PARTIAL_PREVIEW[at++]);
        setDroppedDown(true);
      }, PARTIAL_MS);
    };

    const onPreview = (event: Event) => {
      const command = (event as CustomEvent<PreviewCommand>).detail;
      if (command.style) setStyle(command.style);
      // Phase first, because `recording` resets everything below it — including
      // the one timer both streams share. Starting a stream before that reset
      // has it cancelled by the same command that asked for it.
      if (command.phase) {
        setPhase(command.phase);
        setDismissing(false);
        if (command.phase === "recording") {
          clearInterval(typing);
          setAsk(null);
          setAgent(null);
          setError(null);
          setPartial("");
          setDroppedDown(false);
          setRun((previous) => previous + 1);
        }
      }
      if (command.ask === "open") setAsk(ASK_PREVIEW);
      if (command.ask === "close") {
        clearInterval(typing);
        setAsk(null);
        setAgent(null);
      }
      if (command.ask === "stream") stream();
      if (command.partial === "stream") dictate();
      if (command.agent === "run") {
        setAsk(null);
        setAgent({
          ...AGENT_PREVIEW,
          steps: [AGENT_PREVIEW.steps[0]],
          finished:
            "Sent the release notes to Priya. The reply to Sam is drafted and waiting in Gmail.",
        });
      }
      if (command.agent === "pending") {
        setAsk(null);
        setAgent(AGENT_PREVIEW);
      }
      if (command.agent === "close") setAgent(null);
      if (command.partial === "clear") {
        clearInterval(typing);
        setPartial("");
      }
      if (command.dismissing !== undefined) setDismissing(command.dismissing);
      // Last, so it applies on top of whatever else this command changed.
      if (command.restart) setRun((previous) => previous + 1);
    };

    window.addEventListener(PREVIEW_EVENT, onPreview);
    return () => {
      window.removeEventListener(PREVIEW_EVENT, onPreview);
      clearInterval(typing);
    };
  }, []);

  // An agent run. Its own listener beside the ask one, because they are
  // separate event streams that happen to share a surface.
  useEffect(() => {
    const unlisten = events.agent.listen(({ payload }) => {
      switch (payload.kind) {
        case "runStarted":
          // A new run resets the panel; it is reused, not recreated.
          setAgent({ heard: payload.value, steps: [] });
          break;
        case "toolCall":
          setAgent((r) => r && { ...r, steps: withCall(r.steps, payload.value), status: undefined });
          break;
        case "toolResult":
          setAgent((r) => r && { ...r, steps: withResult(r.steps, payload.value), status: undefined });
          break;
        case "status":
          setAgent((r) => r && { ...r, status: payload.value });
          break;
        case "runFinished":
          // The panel stays up: a run is something you check succeeded, and
          // Rust suppresses the hide the finishing run would otherwise trigger.
          setAgent((r) => r && { ...r, finished: payload.value, status: undefined });
          break;
        case "runError":
          setAgent((r) => r && { ...r, error: payload.value, status: undefined });
          break;
      }
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  // Follow a long run to its newest step, the same as the answer stream.
  useEffect(() => {
    const el = answerRef.current;
    if (el && agent && !agent.finished) el.scrollTop = el.scrollHeight;
  }, [agent]);

  // Escape dismisses the panel. Only while one is up: the window is
  // click-through and unfocused otherwise, so this cannot fire, but the guard
  // keeps a stray keystroke from asking Rust to hide a pill mid-dictation.
  useEffect(() => {
    if (!panelOpen) return;
    const onKey = (e: KeyboardEvent) => {
      // `closeAgent` is not `closeAsk`: it also abandons any pending
      // confirmation, which is what makes Escape read as "no" rather than
      // leaving a write waiting on a channel nobody will answer.
      if (e.key === "Escape") void (agent ? commands.closeAgent() : commands.closeAsk());
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [panelOpen, agent]);

  // Follow the stream to the bottom, and stop once it finishes — the same
  // simpler-than-tracking-manual-scroll choice the old Ask window made.
  useEffect(() => {
    const el = answerRef.current;
    if (el && ask && !ask.done) el.scrollTop = el.scrollHeight;
  }, [ask]);

  // The live transcript replaces the status word while it is running: once
  // there are words, "Listening…" is saying less than they are. Only during
  // `recording` — a partial left over from the capture must not sit under
  // "Transcribing…", which is about the *whole* utterance being re-read.

  // Follow the words down as they arrive. The panel grows to fit until the cap
  // in overlay.css, and after that this is the only thing keeping the newest
  // words on screen.
  useEffect(() => {
    const el = transcriptRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [partial]);

  const label =
    phase === "error" && error
      ? truncate(error, ERROR_CHARS)
      : download !== null
        ? `Downloading model… ${Math.round(download * 100)}%`
        : LABEL[phase];

  return (
    // The exit is a state of the whole surface, not of one style: both have
    // the window held open for them, so both have to use it.
    <div className="overlay-root" ref={root} data-exiting={dismissing || undefined}>
    {/* Rendered whenever wings are the chosen style, and deliberately left
        rendered *underneath* the panel once it has taken over. Both are pure
        black welded to the same cutout, so the panel covers the tabs as it
        grows past them and uncovers them as it collapses — nothing has to
        animate out, and more importantly nothing pinches: hiding the wings on
        hand-over would snap the bar from their width down to the bare cutout
        for the frame before the panel starts growing.
        This relies on the panel always settling wider than the tabs, which
        holds while its floor (notch x 1.5) exceeds their span (notch + 88) —
        true for any cutout wider than 176pt, and a real one is ~180-220. */}
    {style === "wings" && (
      // voice-dict's minimalist overlay: one continuous black bar welded to the
      // notch — the waveform on the left, a recording light on the right, the
      // measured cutout between them. Only ever shown on a screen that has a
      // notch; Rust falls back to the drop pill otherwise (see overlay.rs).
      <div className={`wings wings--${phase}`}>
        <span className="wing wing--left">
          <PhaseSwap phase={phase}>
            <Indicator phase={phase} />
          </PhaseSwap>
        </span>
        {/* The notch itself: solid black, exactly the cutout's width, so the
            bar is continuous across it and the camera notch disappears into
            the strip — the whole point of the blend. */}
        <span className="wing-notch" aria-hidden />
        <span className="wing wing--right">
          {phase === "recording" && (
            <span className="dot dot--recording" aria-hidden />
          )}
        </span>
      </div>
    )}
    {effectiveStyle === "dropPill" && (
      // voice-dict's drop-down pill: a solid-black panel welded to the top,
      // rounded only along the bottom, with the wave and status sitting just
      // below the notch (the __notch spacer is the measured cutout height).
      // Not `--idle` while an answer is up, whatever the phase says: `--idle`
      // is `display: none`, and the run that produced the answer has ended.
      <div
        className={`drop-pill drop-pill--${panelOpen ? "answering" : phase}`}
        ref={panel.surface}
      >
        {/* The panel's whole silhouette. A path and not a border-radius,
            because the two top corners are concave — see lib/notch-shape.ts. */}
        <svg
          className="drop-pill__shape"
          ref={panel.shape}
          preserveAspectRatio="none"
          aria-hidden
        >
          <path ref={panel.path} fill="var(--notch-fill)" />
        </svg>
        <span className="drop-pill__notch" aria-hidden />
        <div
          className={`drop-pill__body${
            panelOpen
              ? " drop-pill__body--ask"
              : transcript
                ? " drop-pill__body--transcript"
                : ""
          }`}
          ref={panel.content}
        >
          {ask ? (
            // Keyed on the question so a new one resets the copied state and
            // the follow-up field.
            <Answer key={ask.question} ask={ask} answerRef={answerRef} />
          ) : agent ? (
            <AgentRun key={agent.heard} run={agent} bodyRef={answerRef} />
          ) : (
            <PhaseSwap phase={phase}>
              <Indicator phase={phase} />
              {/* Deliberately inside PhaseSwap and beside the wave, rather than
                  a layout of its own: the wave stays exactly where it is and
                  only the text beside it changes, so "Listening…" becoming the
                  words is one surface growing rather than a re-layout. */}
              {transcript ? (
                <p className="transcript" ref={transcriptRef}>
                  {partial}
                </p>
              ) : (
                <span className="label" title={error ?? undefined}>
                  {label}
                </span>
              )}
            </PhaseSwap>
          )}
        </div>
      </div>
    )}
    </div>
  );
}

/**
 * The question and its streaming answer, inside the drop-down panel.
 *
 * Nothing here sizes anything: this is the panel's measured content node's only
 * child, so the springs in `useNotchPanel` see it grow through the same
 * ResizeObserver that already tracked the status word, and the panel settles
 * around it. That is the whole reason the answer moved into this window —
 * asking a question *grows* the thing already on screen instead of putting a
 * second one over it.
 */
function Answer({
  ask,
  answerRef,
}: {
  ask: Ask;
  answerRef: React.RefObject<HTMLDivElement | null>;
}) {
  const [typed, setTyped] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // DOM focus only, never a window focus request: `show_panel` already made the
  // overlay key when the answer opened, and it is the only thing that takes
  // focus from the app being dictated into.
  useEffect(() => {
    inputRef.current?.focus();
  }, [ask.question]);

  return (
    <Panel
      title={ask.question}
      bodyRef={answerRef}
      copyText={ask.answer}
      onClose={() => void commands.closeAsk()}
      footer={
        <input
          ref={inputRef}
          className="ask__input"
          // Not "Ask a follow-up": `stream_answer` builds a fresh
          // [system, user] pair with no prior turn, so there is no conversation
          // to follow up on. Promising one and then answering "and what about
          // Friday?" with no context is worse than not offering it.
          placeholder="Ask a question…"
          value={typed}
          // Disabled mid-answer: two streams write Delta into the same panel,
          // and the second one's Question event resets the answer to "" while
          // the first keeps appending — the two answers splice together.
          disabled={!ask.done}
          onChange={(e) => setTyped(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== "Enter" || !typed.trim() || !ask.done) return;
            // Cleared here rather than waiting for the backend's `question`
            // event to echo back: until that lands the text sits in the field
            // and is trivially re-submittable.
            const question = typed;
            setTyped("");
            // The same backend entry point the spoken trigger uses, so the two
            // paths cannot answer differently.
            void commands.askText(question);
          }}
        />
      }
    >
      {ask.error ? (
        <p className="ask__error">{ask.error}</p>
      ) : ask.answer ? (
        <MessageResponse>{ask.answer}</MessageResponse>
      ) : (
        <span className="dot dot--transcribing" aria-hidden />
      )}
    </Panel>
  );
}


/**
 * An agent run, in the notch panel: what was heard, the tools it resolved to,
 * the approval a write has to pass, and the closing answer.
 *
 * Ported from the Agent window rather than rewritten — the flow and its
 * comments were right, only the surface changed. The one part that did get
 * rewritten is the confirmation, which used to be a shadcn Alert inside a
 * card: a bordered box within a black panel reads as a card someone dropped
 * in, where this surface wants the approval to be part of it.
 */
function AgentRun({
  run,
  bodyRef,
}: {
  run: Run;
  bodyRef: React.RefObject<HTMLDivElement | null>;
}) {
  const pending = pendingStep(run);

  const confirm = (approved: boolean) => void commands.confirmAgent(approved);

  return (
    <Panel
      title={run.heard}
      bodyRef={bodyRef}
      // Nothing to copy until the run has said something.
      copyText={run.finished || undefined}
      onClose={() => void commands.closeAgent()}
      footer={
        <span className="agent__trail">
          {run.steps.length > 0
            ? labelFor(
                run.steps[run.steps.length - 1].name,
                run.steps[run.steps.length - 1].args,
              )
            : ""}
        </span>
      }
    >
      {/* Before any tool call resolves, we're still parsing the transcript. */}
      {run.steps.length === 0 && !run.error && !run.status && (
        <p className="agent__status">
          <LoaderCircleIcon className="size-4 animate-spin" />
          <Shimmer>Understanding…</Shimmer>
        </p>
      )}

      {/* One Tool per call. The state mapping is exact — AI Elements happens
          to name the same five outcomes an Aloud run has, including the denied
          one a cancelled confirmation produces. */}
      {run.steps.map((step, i) => (
        <Tool
          key={`${step.name}-${i}`}
          // Open on the step that is waiting for a decision, and on a failure.
          // Everything else stays folded: a finished run is a list of things
          // that worked, and expanding all of them buries the answer.
          defaultOpen={step === pending || step.result?.success === false}
        >
          <ToolHeader
            type={`tool-${step.name}`}
            state={stateFor(step, step === pending)}
            // The curated label, not the raw tool name. `tool-views` turns
            // GMAIL_SEND_EMAIL and its arguments into "Sending an email to
            // priya", and that is the whole reason it exists.
            title={labelFor(step.name, step.args)}
          />
          <ToolContent>
            {/* The registered view first, because it is the readable one; the
                raw parameters sit under it for when they matter. */}
            {bodyFor(step.name, step.args)}
            <ToolInput input={step.args} />
            {step.result && (
              <ToolOutput
                output={step.result.success ? step.result.message : undefined}
                errorText={step.result.success ? undefined : step.result.message}
              />
            )}
          </ToolContent>
        </Tool>
      ))}

      {pending && (
        <Confirmation approval={{ id: pending.name }} state="approval-requested">
          <ConfirmationTitle>
            {labelFor(pending.name, pending.args)} — this writes to {pending.app}.
          </ConfirmationTitle>
          <ConfirmationRequest>
            {/* Approving a write without seeing what it will do is not a gate,
                so the body shows here even though the step list is quiet. */}
            <div className="mb-3">{bodyFor(pending.name, pending.args)}</div>
            <ConfirmationActions>
              <ConfirmationAction variant="outline" onClick={() => confirm(false)}>
                Cancel
              </ConfirmationAction>
              <ConfirmationAction autoFocus onClick={() => confirm(true)}>
                Confirm
              </ConfirmationAction>
            </ConfirmationActions>
          </ConfirmationRequest>
        </Confirmation>
      )}

      {run.status && (
        <p className="agent__status">
          <LoaderCircleIcon className="size-4 animate-spin" />
          <Shimmer>{run.status}</Shimmer>
        </p>
      )}

      {/* The run's closing answer — the whole point of the run. */}
      {run.finished && <MessageResponse>{run.finished}</MessageResponse>}
      {run.error && <p className="ask__error">{run.error}</p>}
    </Panel>
  );
}

/**
 * An Aloud step in the vocabulary `ToolHeader` speaks.
 *
 * The five states line up one-for-one, which is the reason this is a lookup
 * and not an adapter: a call is running, waiting on the user, done, denied by
 * that user, or failed.
 */
function stateFor(step: Step, pending: boolean): ToolUIPart["state"] {
  if (step.result) return step.result.success ? "output-available" : "output-error";
  if (step.confirmed === false) return "output-denied";
  return pending ? "approval-requested" : "input-available";
}

/**
 * The chrome both Ask and Agent draw inside the notch panel: what was heard,
 * a rule, the scrolling body, and a footer.
 *
 * This is what `components/ui/panel-shell` was for the two windows that no
 * longer exist. It is not that component moved here — that one is a shadcn
 * Card sized to a window, and none of that survives the move to a surface
 * welded to the notch. What is shared is the shape, so only the shape is.
 */
function Panel({
  title,
  bodyRef,
  /** Omit to hide the copy button — there is nothing worth copying yet. */
  copyText,
  onClose,
  footer,
  children,
}: {
  title: string;
  bodyRef?: React.RefObject<HTMLDivElement | null>;
  copyText?: string;
  onClose: () => void;
  /** Sits left of copy/close. */
  footer?: ReactNode;
  children: ReactNode;
}) {
  const [copied, setCopied] = useState(false);

  return (
    <div className="ask">
      <p className="ask__question">{title}</p>
      <div className="ask__answer" ref={bodyRef}>
        {children}
      </div>
      <div className="ask__footer">
        {footer}
        {copyText !== undefined && (
          <button
            className="ask__button"
            aria-label={copied ? "Copied" : "Copy"}
            onClick={() =>
              void navigator.clipboard.writeText(copyText).then(() => setCopied(true))
            }
          >
            {copied ? <CheckIcon size={14} /> : <CopyIcon size={14} />}
          </button>
        )}
        <button className="ask__button" aria-label="Close" onClick={onClose}>
          <XIcon size={14} />
        </button>
      </div>
    </div>
  );
}

/**
 * Cross-fades its contents whenever the phase changes.
 *
 * The `key` is the whole mechanism: React remounts on a new phase, so the CSS
 * entrance in overlay.css runs again without the usual reflow-to-restart-a-
 * keyframe dance. Cheap here because the subtree is a dot (or five bars) and a
 * word — and the bars only exist in one phase, so nothing mid-wave is thrown
 * away by the remount.
 */
function PhaseSwap({ phase, children }: { phase: Phase; children: ReactNode }) {
  return (
    <span key={phase} className="phase-swap">
      {children}
    </span>
  );
}

/** The wave while recording, a status dot otherwise. Shared by both styles. */
function Indicator({ phase }: { phase: Phase }) {
  if (phase !== "recording") {
    return <span className={`dot dot--${phase}`} aria-hidden />;
  }
  // A continuous wave the voice scales rather than drives. CSS owns the
  // motion; these two custom properties only give each bar its place in the
  // row and its height ceiling.
  return (
    <span className="bars" aria-hidden>
      {BAR_PEAKS.map((peak, i) => (
        <i key={i} style={{ "--i": i, "--peak": peak } as React.CSSProperties} />
      ))}
    </span>
  );
}

/** Cuts on a char boundary so a multi-byte character is never split. */
function truncate(text: string, max: number): string {
  const flat = text.replace(/\s+/g, " ").trim();
  return [...flat].length <= max ? flat : `${[...flat].slice(0, max).join("")}…`;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
