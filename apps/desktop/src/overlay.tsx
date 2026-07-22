import React, {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import ReactDOM from "react-dom/client";
import {
  CheckIcon,
  CircleDashedIcon,
  CopyIcon,
  LoaderCircleIcon,
  PlusIcon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react";
import { commands, events, type OverlayStyle, type Phase } from "./lib/bindings";
import { MessageResponse } from "@/components/ai-elements/message";
import { Shimmer } from "@/components/ai-elements/shimmer";
import {
  groupSteps,
  pendingStep,
  withCall,
  withResult,
  withoutPlanned,
  type Run,
  type Step,
} from "./lib/agent-steps";
import {
  askFor,
  bodyFor,
  editsFor,
  labelFor,
  type ToolArgs,
} from "./lib/tool-views";
import { notchPath } from "./lib/notch-shape";
import { PREVIEW_EVENT, type PreviewCommand } from "./lib/overlay-preview";
import { createSpring } from "./lib/spring";
// Streamdown styles the streamed answer with Tailwind utilities — see the
// `@source` note in index.css.
import "./index.css";
import "./styles/overlay.css";

const LABEL: Record<Phase, string> = {
  idle: "",
  recording: "Listening…",
  transcribing: "Transcribing…",
  done: "Pasted",
  noSpeech: "No speech detected",
  // Only shown if an error arrives without a message; Rust normally sends one.
  error: "Failed",
};

/** A long error still has to fit one line on a small pill. */
const ERROR_CHARS = 90;

/**
 * Bars in the listening wave. With the per-bar delay in overlay.css this sets
 * how much of a wave fits across the row — change one, move the other.
 */
const BAR_COUNT = 5;

/**
 * Per-bar height ceiling: a raised cosine, tallest in the middle, so the row
 * reads as a waveform and not a ribbon. The floor is 0.6, not 0, because it
 * scales the trough too — lower crushed the outer bars to a 1px dash at rest.
 */
const BAR_PEAKS = Array.from({ length: BAR_COUNT }, (_, i) => {
  const position = i / (BAR_COUNT - 1); // 0…1 across the row
  return 0.6 + 0.4 * Math.sin(Math.PI * position);
});

/** Only for a stylesheet that has not applied yet; motion.css owns the real values. */
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
 * How the corner radii open up as the panel grows past the cutout. Derived from
 * the *current* height, not the target, so they interpolate with the box rather
 * than jumping to their final value on the first frame.
 */
const RADIUS = { topGrowth: 0.12, topMax: 16, bottomGrowth: 0.14, bottomMax: 22 };

/** Only for the `?style=pill` browser preview, where no notch is measured. */
const NOTCH_FALLBACK = { width: 200, height: 32 };

/**
 * Minimum panel width, as a multiple of the cutout, so it is never narrower
 * than the hole it hangs from. Wider content still wins.
 */
const PANEL_WIDTH_FACTOR = 1.5;

/**
 * Grows the drop-down panel out of the notch and shrinks it back in.
 *
 * The one surface that is not a CSS transition: its size is retargeted
 * mid-flight three times a dictation, and a baked curve redirected halfway
 * restarts from zero velocity — hence `lib/spring.ts`. Writes the box and the
 * shape's `d` straight onto the nodes, so no React render sits between frames.
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
    // Nothing on screen yet (the pill is `display: none` until the first run);
    // measuring here would set the target from a zero-width box.
    if (!contentElement.offsetWidth) return;

    // Read once per rebuild, not per retarget: it is a style recalc, and
    // `remeasure` fires once a frame while an answer streams.
    const PANEL = readPanelSpring(surfaceElement);

    let base = { width: NOTCH_FALLBACK.width, height: NOTCH_FALLBACK.height };
    let target = base;
    let baseRadius = NOTCH_FALLBACK.height / 3;
    let collapsed = false;

    // getComputedStyle forces a style recalc, so it is called when the layout
    // changes and never from inside a frame.
    const measure = () => {
      const computed = getComputedStyle(surfaceElement);
      // A screen with no notch reports a real zero; `||` would conflate that
      // with the variable being absent and invent a cutout on an iMac. Only a
      // non-number falls back.
      const measured = (name: string, fallback: number) => {
        const value = parseFloat(computed.getPropertyValue(name));
        return Number.isFinite(value) ? value : fallback;
      };
      const notchWidth = measured("--notch-w", NOTCH_FALLBACK.width);
      const notchHeight = measured("--notch-h", NOTCH_FALLBACK.height);

      // The cutout's own rounding; every other radius is measured from it.
      baseRadius = notchHeight / 3;
      base = { width: notchWidth, height: notchHeight };
      const settledHeight = notchHeight + contentElement.offsetHeight;
      target = {
        // The body sits between the two concave shoulders, so the panel is
        // wider than its content by exactly that much — measured at the
        // *settled* top radius. Never narrower than the cutout, or the physical
        // camera bump sticks out either side.
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
      // Floored at the cutout: the spring rings a fifth of its travel past the
      // target, which on the way back in undershoots clean through the 32pt
      // cutout and springs back out. Only what is drawn is clamped — the spring
      // keeps its real state, so the motion's timing is unchanged.
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
 */
type Ask = {
  question: string;
  answer: string;
  /** Set instead of an answer when the request failed. */
  error: string | null;
  /** The stream has finished — stops the follow-to-bottom and re-arms input. */
  done: boolean;
};

/** Dev/QA: `?ask` / `?ask=<ms>` reaches the answer state without a mic. */
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
 * A dictation preview, as whisper produces one: each entry is the *whole*
 * transcript at that moment, so it can go backwards. The two shortenings are
 * deliberate — a revision is what makes the panel shrink mid-flight.
 */
const PARTIAL_PREVIEW = [
  "So the",
  "So the thing about",
  "So the thing about springs is",
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
 * One tab in the notch's strip. Tab 0 is the live ask/agent panel; the rest are
 * blank until we decide what they hold. `closing` is set instead of removing
 * the tab, so its exit has something to play on — see `closeTab`.
 */
type Tab = { id: number; blank: boolean; closing?: boolean };

/** The strip a run starts with: just the panel the dictation opened. */
const FIRST_TABS: Tab[] = [{ id: 0, blank: false }];

/**
 * Where `+` stops. The strip has the answer body's 480pt and no more, and past
 * this every label is ellipsed to two characters — a notch panel is not a
 * browser. ponytail: a hard cap, not overflow scrolling; if tabs ever carry
 * enough to want more, the strip needs its own overflow affordance anyway.
 */
const MAX_TABS = 5;

/**
 * The recording pill. Rust drives the window's visibility, so this only ever
 * renders the current phase.
 */
function Overlay() {
  // Dev/QA: `?phase=recording` renders the pill statically in a browser. No
  // query string in the real app, so this is idle until Rust drives it.
  const preview = new URLSearchParams(location.search).get("phase") as Phase | null;
  const [phase, setPhase] = useState<Phase>(preview ?? "idle");
  // Only what is drawn before Rust's first style event lands, so it has to
  // match the default in `Settings` rather than be a second opinion about it.
  const [style, setStyle] = useState<OverlayStyle>(
    (new URLSearchParams(location.search).get("style") as OverlayStyle) ??
      "dropPill",
  );
  const [error, setError] = useState<string | null>(null);
  const [download, setDownload] = useState<number | null>(null);
  // The words so far while the live preview is on. Feedback only — never
  // pasted, and it revises itself as a sentence lands.
  const [partial, setPartial] = useState("");
  // Latched once a run has produced words; see `effectiveStyle`.
  const [droppedDown, setDroppedDown] = useState(false);
  const [ask, setAsk] = useState<Ask | null>(null);
  const [agent, setAgent] = useState<Run | null>(null);
  // Either of them owns the panel; almost everything below only cares that one does.
  const panelOpen = ask !== null || agent !== null;
  // A ref, not the state it mirrors: the phase listener subscribes once, and
  // depending on `panelOpen` would rebuild it on every streamed token.
  const openRef = useRef(panelOpen);
  openRef.current = panelOpen;
  // Rust tells us the window is about to go away, so the panel can collapse
  // back into the notch while there is still a surface to draw it on.
  const [dismissing, setDismissing] = useState(false);
  // Bumped once per dictation: the panel keys its springs off this, and the
  // phase alone cannot say "a new run began".
  const [run, setRun] = useState(0);
  const [tabs, setTabs] = useState<Tab[]>(FIRST_TABS);
  const [activeTab, setActiveTab] = useState(0);
  const nextTabId = useRef(1);
  // A new dictation is a new session, so its extra tabs go with it. Keyed on
  // `run` rather than the phase because both the real and the preview paths
  // bump it, which makes this the single place the reset has to live.
  useEffect(() => {
    setTabs(FIRST_TABS);
    setActiveTab(0);
  }, [run]);

  // The cap is enforced here rather than in each button, so no caller can get
  // past it — the buttons only hide themselves so the strip stops offering.
  // The cap is enforced here rather than in each button, so no caller can get
  // past it — the buttons only hide themselves so the strip stops offering.
  const addTab = useCallback(() => {
    if (tabs.length >= MAX_TABS) return;
    const id = nextTabId.current++;
    setTabs((current) => [...current, { id, blank: true }]);
    setActiveTab(id);
  }, [tabs.length]);

  // Marked, not removed: unmounting here would cut the exit off mid-frame. The
  // removal is `dropTab`, downstream of the animation finishing.
  const closeTab = useCallback((id: number) => {
    setTabs((current) =>
      current.map((tab) => (tab.id === id ? { ...tab, closing: true } : tab)),
    );
    setActiveTab((active) => (active === id ? 0 : active));
  }, []);

  const dropTab = useCallback(
    (id: number) => setTabs((current) => current.filter((tab) => tab.id !== id)),
    [],
  );
  // Dev/QA: `?dismiss=800` plays the exit 800ms in — otherwise only reachable
  // from a Rust event, and it is the one path here that races.
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
  // Only during `recording`: a partial left over from the capture must not sit
  // under "Transcribing…", which is about the whole utterance being re-read.
  const transcript = !panelOpen && phase === "recording" && partial.trim().length > 0;

  /**
   * The style actually drawn: a wing has nowhere to put a sentence, so the
   * moment there are words it hands over to the drop-down panel. `droppedDown`
   * latches for the rest of the run rather than tracking the words — the
   * transcript is gone by `transcribing`, and springing back up to a menu-bar
   * tab there would undo the drop just watched.
   */
  const effectiveStyle: OverlayStyle =
    style === "wings" && (droppedDown || panelOpen) ? "dropPill" : style;

  // Rust tests the pointer against this box to decide when the window takes
  // clicks (`track_cursor` in overlay.rs). Reported only when it changes: the
  // target moves roughly once a frame while an answer streams.
  const reported = useRef("");
  const reportBox = useCallback((width: number, height: number) => {
    const box = `${Math.round(width)}x${Math.round(height)}`;
    if (box === reported.current) return;
    reported.current = box;
    // `invoke` throws synchronously in the browser preview, and this runs
    // inside the panel's own layout pass — letting it escape kills the overlay.
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

  // Levels land every 50ms; writing --level onto the node keeps 20 renders a
  // second out of React, which would rebuild the pill mid-wave. Null = silence.
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
      // window stays up for the exit and needs something on it to animate. Hold
      // the last real phase and start collapsing instead — blanking here is
      // what made a no-speech run cut off dead while a pasted one animated.
      if (e.payload === "idle") {
        // …unless an answer is on screen: both Ask paths end the run on
        // `idle`, which is exactly when the answer wants reading. Rust
        // suppresses the matching hide for the same reason (ANSWERING).
        if (!openRef.current) setDismissing(true);
        return;
      }

      setPhase(e.payload);
      // A phase arriving means the window is staying up, cancelling an exit.
      setDismissing(false);
      if (e.payload === "recording") {
        setError(null);
        // A dictation started over the top of an answer takes the panel back.
        setAsk(null);
        setAgent(null);
        setPartial("");
        setDroppedDown(false);
        setRun((previous) => previous + 1);
      }
    });
    const unlistenDismiss = events.overlayDismiss.listen(() => setDismissing(true));
    const unlistenError = events.overlayError.listen((e) => setError(e.payload));
    const unlistenStyle = events.overlayStyle.listen((e) => {
      setStyle(e.payload.style);
      // The measured cutout, as vars the CSS reads, so the wings weld to it
      // rather than straddle a guess. Zero for a pill.
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
    // Rust sends an empty string to say the live preview is over.
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

  // The tuning page's hatch; see PreviewCommand. Inert in the real app.
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
      // Phase first: `recording` resets everything below, including the timer
      // both streams share, so a stream started before it would be cancelled.
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
      if (command.agent === "planned") {
        setAsk(null);
        setAgent({
          ...AGENT_PREVIEW,
          steps: [AGENT_PREVIEW.steps[0]],
          planned: [
            { name: "GMAIL_CREATE_EMAIL_DRAFT", app: "Gmail" },
            { name: "GOOGLECALENDAR_FIND_EVENT", app: "Google Calendar" },
          ],
        });
      }
      // The editable gate, as it arrives after both lookups came back empty:
      // a name and no address, which is exactly the state the inputs exist for.
      if (command.agent === "contact") {
        setAsk(null);
        setAgent({
          heard: "Email Genghis Khan the release notes",
          steps: [
            // The reported sequence: one lookup that cost three calls. The two
            // adjacent inbox reads collapse; the contacts search stays its own
            // row because it reads differently.
            {
              name: "GOOGLECONTACTS_SEARCH_CONTACTS",
              app: "Google Contacts",
              args: { query: "Genghis Khan" },
              requiresConfirmation: false,
              result: { success: true, message: "no results" },
            },
            {
              name: "GMAIL_FETCH_EMAILS",
              app: "Gmail",
              args: { query: "Genghis Khan" },
              requiresConfirmation: false,
              result: { success: true, message: "no results" },
            },
            {
              name: "GMAIL_FETCH_EMAILS",
              app: "Gmail",
              args: { query: "from:Genghis" },
              requiresConfirmation: false,
              result: { success: true, message: "no results" },
            },
            {
              name: "GOOGLECONTACTS_CREATE_CONTACT",
              app: "Google Contacts",
              // Stringified, because that is how `display_args` delivers a
              // nested argument to the panel.
              args: {
                names: JSON.stringify([
                  { givenName: "Genghis Khan", displayName: "Genghis Khan" },
                ]),
                emailAddresses: JSON.stringify([]),
                personFields: "names,emailAddresses",
              },
              requiresConfirmation: true,
            },
          ],
        });
      }
      // The model waiting on the user; the run is suspended in Rust for as
      // long as this is up.
      if (command.agent === "asked") {
        setAsk(null);
        setAgent({
          heard: "Send hi to Charlie Yang",
          steps: [
            {
              name: "GOOGLECONTACTS_SEARCH_CONTACTS",
              app: "Google Contacts",
              args: { query: "Charlie Yang" },
              requiresConfirmation: false,
              result: { success: true, message: "no results" },
            },
            {
              name: "GMAIL_FETCH_EMAILS",
              app: "Gmail",
              args: { query: "Charlie Yang" },
              requiresConfirmation: false,
              result: { success: true, message: "no results" },
            },
          ],
          question: {
            question: "What is Charlie Yang's email address?",
            options: [],
          },
        });
      }
      // The same channel, asking a question whose answers are known — which is
      // what a lookup with several matches should do instead of picking.
      if (command.agent === "choose") {
        setAsk(null);
        setAgent({
          heard: "Send hi to Sam",
          steps: [
            {
              name: "GOOGLECONTACTS_SEARCH_CONTACTS",
              app: "Google Contacts",
              args: { query: "Sam" },
              requiresConfirmation: false,
              result: { success: true, message: "3 results" },
            },
          ],
          question: {
            question: "Which Sam did you mean?",
            options: [
              "sam.patel@example.com",
              "samantha.reeve@example.com",
              "s.okafor@contractors.example.com",
            ],
          },
        });
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

  // An agent run. Its own listener: a separate event stream, a shared surface.
  useEffect(() => {
    const unlisten = events.agent.listen(({ payload }) => {
      switch (payload.kind) {
        case "runStarted":
          // A new run resets the panel; it is reused, not recreated.
          setAgent({ heard: payload.value, steps: [] });
          break;
        case "planned":
          setAgent((r) => r && { ...r, planned: payload.value });
          break;
        case "toolCall":
          setAgent(
            (r) =>
              r && {
                ...r,
                steps: withCall(r.steps, payload.value),
                planned: withoutPlanned(r.planned, payload.value.name),
                status: undefined,
                question: undefined,
              },
          );
          break;
        case "toolResult":
          setAgent((r) => r && { ...r, steps: withResult(r.steps, payload.value), status: undefined });
          break;
        case "asked":
          setAgent((r) => r && { ...r, question: payload.value, status: undefined });
          break;
        case "status":
          setAgent((r) => r && { ...r, status: payload.value });
          break;
        case "runFinished":
          // The panel stays up: a run is something you check succeeded, and
          // Rust suppresses the hide the finishing run would otherwise trigger.
          setAgent(
            (r) =>
              r && {
                ...r,
                finished: payload.value,
                status: undefined,
                planned: undefined,
                question: undefined,
              },
          );
          break;
        case "runError":
          setAgent(
            (r) => r && {
              ...r,
              error: payload.value,
              status: undefined,
              planned: undefined,
              question: undefined,
            },
          );
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

  // Escape dismisses the panel. Guarded on `panelOpen` so a stray keystroke
  // cannot ask Rust to hide a pill mid-dictation.
  useEffect(() => {
    if (!panelOpen) return;
    const onKey = (e: KeyboardEvent) => {
      // `closeAgent` also abandons a pending confirmation, so Escape reads as
      // "no" rather than leaving a write waiting on a channel nobody answers.
      if (e.key === "Escape") void (agent ? commands.closeAgent() : commands.closeAsk());
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [panelOpen, agent]);

  // Follow the stream to the bottom; simpler than tracking manual scroll.
  useEffect(() => {
    const el = answerRef.current;
    if (el && ask && !ask.done) el.scrollTop = el.scrollHeight;
  }, [ask]);

  // Follow the words down as they arrive: past the height cap in overlay.css
  // this is the only thing keeping the newest ones on screen.
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
    // The exit is a state of the whole surface, not of one style.
    <div className="overlay-root" ref={root} data-exiting={dismissing || undefined}>
    {/* Left rendered *underneath* the panel once it takes over: hiding the
        wings on hand-over snaps the bar down to the bare cutout for a frame.
        Relies on the panel settling wider than the tabs — its floor
        (notch x 1.5) beats their span (notch + 88) for any cutout over 176pt. */}
    {style === "wings" && (
      // One continuous black bar welded to the notch. Only shown on a screen
      // that has one; Rust falls back to the drop pill otherwise (overlay.rs).
      <div className={`wings wings--${phase}`}>
        <span className="wing wing--left">
          <PhaseSwap phase={phase}>
            <Indicator phase={phase} />
          </PhaseSwap>
        </span>
        {/* The cutout itself: solid black at exactly its width, so the bar
            reads as continuous across it. */}
        <span className="wing-notch" aria-hidden />
        <span className="wing wing--right">
          {phase === "recording" && (
            <span className="dot dot--recording" aria-hidden />
          )}
        </span>
      </div>
    )}
    {effectiveStyle === "dropPill" && (
      // A solid-black panel welded to the top; the __notch spacer is the
      // measured cutout height. Never `--idle` while an answer is up, whatever
      // the phase says — `--idle` is `display: none`.
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
          {panelOpen ? (
            <>
              <TabStrip
                tabs={tabs}
                active={activeTab}
                title={ask?.question ?? agent?.heard ?? "Aloud"}
                onSelect={setActiveTab}
                onClose={closeTab}
                onClosed={dropTab}
                onAdd={addTab}
              />
              {/* Keyed so switching back to a tab replays its entrance rather
                  than having the content appear already settled. */}
              {tabs.find((tab) => tab.id === activeTab)?.blank ? (
                <div className="tab-blank" key={activeTab} />
              ) : ask ? (
                // Keyed on the question so a new one resets copy state and input.
                <Answer
                  key={ask.question}
                  ask={ask}
                  answerRef={answerRef}
                  onAddTab={tabs.length < MAX_TABS ? addTab : undefined}
                />
              ) : agent ? (
                <AgentRun
                  key={agent.heard}
                  run={agent}
                  bodyRef={answerRef}
                  onAddTab={tabs.length < MAX_TABS ? addTab : undefined}
                />
              ) : null}
            </>
          ) : (
            <PhaseSwap phase={phase}>
              <Indicator phase={phase} />
              {/* Inside PhaseSwap and beside the wave, not a layout of its
                  own, so only the text changes and the wave stays put. */}
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
 * The strip across the top of the notch panel. Terminal-style: the run that
 * opened the panel is the first tab, `+` puts a blank one beside it.
 *
 * Sizes nothing — it lives inside the fixed-width answer body, so adding a tab
 * moves the panel's shoulders not at all and only the swapped body retargets
 * the height spring.
 */
function TabStrip({
  tabs,
  active,
  /** What tab 0 is called: the question asked, or what the agent heard. */
  title,
  onSelect,
  onClose,
  onClosed,
  onAdd,
}: {
  tabs: Tab[];
  active: number;
  title: string;
  onSelect: (id: number) => void;
  onClose: (id: number) => void;
  onClosed: (id: number) => void;
  onAdd: () => void;
}) {
  return (
    <div className="tabs">
      {tabs.map((tab, i) => (
        // The wrapper is what collapses; the chip inside it is what is clipped.
        <div
          key={tab.id}
          className="tab"
          data-motion={tab.closing ? "out" : "in"}
          // Guarded on the target: animationend bubbles, and an animation added
          // inside a chip later would otherwise delete the tab under it.
          onAnimationEnd={(e) =>
            tab.closing && e.target === e.currentTarget && onClosed(tab.id)
          }
        >
          <div className="tab__chip" data-active={tab.id === active || undefined}>
            <button className="tab__select" onClick={() => onSelect(tab.id)}>
              {/* Numbered by position, not by id, so the strip always reads
                  1..n after a close — the same thing a terminal does. Four
                  tabs all called "New Tab" are unswitchable-between. */}
              {tab.blank ? `Tab ${i + 1}` : title}
            </button>
            {/* Tab 0 is the run itself; closing it would leave the panel with
                nothing to show but no way back. Escape still closes the lot. */}
            {tab.id !== 0 && (
              <button
                className="tab__close"
                aria-label="Close tab"
                onClick={() => onClose(tab.id)}
              >
                <XIcon size={11} />
              </button>
            )}
          </div>
        </div>
      ))}
      {tabs.length < MAX_TABS && (
        <button className="tab__add" aria-label="New tab" title="New tab" onClick={onAdd}>
          {/* Heavier than the default 2 at this size — a hairline plus on black
              disappears into the panel. */}
          <PlusIcon size={15} strokeWidth={2.5} />
        </button>
      )}
    </div>
  );
}

/**
 * The question and its streaming answer, inside the drop-down panel. Nothing
 * here sizes anything: it is the measured content node's only child, so the
 * springs in `useNotchPanel` settle around it through the ResizeObserver.
 */
function Answer({
  ask,
  answerRef,
  onAddTab,
}: {
  ask: Ask;
  answerRef: React.RefObject<HTMLDivElement | null>;
  onAddTab?: () => void;
}) {
  const [typed, setTyped] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // DOM focus only, never a window focus request — `show_panel` already made
  // the overlay key, and it is the only thing that takes focus from the app.
  useEffect(() => {
    inputRef.current?.focus();
  }, [ask.question]);

  return (
    <Panel
      title={ask.question}
      bodyRef={answerRef}
      copyText={ask.answer}
      onClose={() => void commands.closeAsk()}
      onAddTab={onAddTab}
      footer={
        <input
          ref={inputRef}
          className="ask__input"
          // Not "Ask a follow-up": `stream_answer` builds a fresh
          // [system, user] pair, so there is no conversation to follow up on.
          placeholder="Ask a question…"
          value={typed}
          // Disabled mid-answer: two streams write Delta into the same panel,
          // and the second one's Question event resets the answer to "" while
          // the first keeps appending — the two answers splice together.
          disabled={!ask.done}
          onChange={(e) => setTyped(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== "Enter" || !typed.trim() || !ask.done) return;
            // Cleared here, not on the backend's echo: until that lands the
            // text sits in the field and is trivially re-submittable.
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
 */
function AgentRun({
  run,
  bodyRef,
  onAddTab,
}: {
  run: Run;
  bodyRef: React.RefObject<HTMLDivElement | null>;
  onAddTab?: () => void;
}) {
  const pending = pendingStep(run);

  return (
    <Panel
      title={run.heard}
      bodyRef={bodyRef}
      // Nothing to copy until the run has said something.
      copyText={run.finished || undefined}
      onClose={() => void commands.closeAgent()}
      onAddTab={onAddTab}
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
          {/* `as="span"`: Shimmer renders a <p> by default, and a <p> inside a
              <p> is invalid — React closes the outer one early and the layout
              silently breaks. */}
          <Shimmer as="span">Understanding…</Shimmer>
        </p>
      )}

      {/* One line per tool: what is running and how it went. The arguments are
          deliberately not shown here — they are a wall of JSON, and the only
          moment they matter is when approving a write, below. */}
      {(run.steps.length > 0 || (run.planned?.length ?? 0) > 0) && (
        <ul className="agent__steps">
          {/* Keyed on the row's own text, so only steps the user could not have
              told apart collapse — see `groupSteps`. */}
          {groupSteps(run.steps, (s) => rowKey(s, s === pending)).map(
            ({ step, count }, i) => (
            <li key={`${step.name}-${i}`}>
              <span className="agent__step">
                <StepIcon step={step} pending={step === pending} />
                <span className="agent__name">{labelFor(step.name, step.args)}</span>
                {count > 1 && <span className="agent__count">×{count}</span>}
                <span className="agent__state">
                  {stepStatus(step, step === pending)}
                </span>
              </span>
              {/* Only what this tool is worth showing, and never for the step
                  awaiting a decision — the gate below shows that one, and
                  rendering both put the same draft on screen twice. */}
              {step !== pending && (
                <span className="agent__body">{bodyFor(step.name, step.args)}</span>
              )}
            </li>
            ),
          )}
          {/* What has not happened yet, in the same list so the run reads in
              order rather than as two stacked sections. */}
          {run.planned?.map((next, i) => (
            <li key={`planned-${next.name}-${i}`} className="agent__step--planned">
              <span className="agent__step">
                <CircleDashedIcon className="agent__icon" size={14} />
                {/* No arguments yet — they arrive with the call. */}
                <span className="agent__name">{labelFor(next.name, {})}</span>
                <span className="agent__state">queued</span>
              </span>
            </li>
          ))}
        </ul>
      )}

      {/* Keyed on the step, so a second gate in one run starts from its own
          arguments instead of inheriting the first one's draft. */}
      {pending && <ConfirmGate key={`${pending.name}-${run.steps.length}`} step={pending} />}

      {/* Keyed on the question so a second one clears the first one's answer. */}
      {run.question && !pending && (
        <AskGate key={run.question.question} asked={run.question} />
      )}

      {run.status && (
        <p className="agent__status">
          <LoaderCircleIcon className="size-4 animate-spin" />
          <Shimmer as="span">{run.status}</Shimmer>
        </p>
      )}

      {run.finished && <MessageResponse>{run.finished}</MessageResponse>}
      {run.error && <p className="ask__error">{run.error}</p>}
    </Panel>
  );
}

/**
 * The model asking the user for something no tool could give it.
 *
 * The run is suspended in Rust for as long as this is on screen, so it is the
 * one place the panel takes input that is not a decision. Dismissing answers
 * "nothing", which the loop reports as unanswered — the model then closes the
 * run rather than retrying into the same wall.
 */
function AskGate({ asked }: { asked: { question: string; options: string[] } }) {
  const [typed, setTyped] = useState("");
  const answer = (text: string | null) => void commands.answerAgent(text);

  // A choice, not a blank. Offered whenever the model knows the candidates —
  // which of three Sams, which calendar — because picking cannot be misspelled
  // and a spoken app should never make someone type an address they can see.
  if (asked.options.length > 0) {
    return (
      <div className="agent__confirm" role="group">
        <p className="agent__confirm-title">{asked.question}</p>
        <div className="agent__options">
          {asked.options.map((option) => (
            <button
              className="agent__option"
              key={option}
              onClick={() => answer(option)}
            >
              {option}
            </button>
          ))}
        </div>
        <div className="agent__actions">
          <button className="agent__action" onClick={() => answer(null)}>
            None of these
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="agent__confirm" role="group">
      <p className="agent__confirm-title">{asked.question}</p>
      <form
        className="agent__ask"
        onSubmit={(e) => {
          e.preventDefault();
          if (typed.trim()) answer(typed.trim());
        }}
      >
        <input
          className="ask__input"
          autoFocus
          value={typed}
          placeholder="Type your answer…"
          onChange={(e) => setTyped(e.target.value)}
        />
        <button className="agent__action" type="button" onClick={() => answer(null)}>
          Skip
        </button>
        <button
          className="agent__action agent__action--go"
          type="submit"
          disabled={!typed.trim()}
        >
          Send
        </button>
      </form>
    </div>
  );
}

/**
 * The gate a write passes before it runs.
 *
 * Most tools show what they are about to do and take a yes. A few declare
 * editable fields (`edits` in tool-views) and show inputs instead — the agent
 * reaches the contact gate precisely because it could not find an address, so
 * refusing is the only other answer it could give.
 */
function ConfirmGate({ step }: { step: Step }) {
  const fields = editsFor(step.name);
  const [draft, setDraft] = useState<ToolArgs>(step.args);
  // A form is a demand, and the agent reaches this one on its own initiative.
  // Ask first; the fields only appear for someone who said yes.
  const [filling, setFilling] = useState(false);
  const asking = fields.length > 0 && !filling;

  // Only what the user could have changed. Rust folds it back into the real
  // nested arguments; anything untouched keeps the value the model chose.
  const confirm = (approved: boolean) =>
    void commands.confirmAgent(
      approved,
      approved && fields.length > 0 ? asStrings(draft) : null,
    );

  return (
    <div className="agent__confirm" role="group">
      <p className="agent__confirm-title">
        {asking && askFor(step.name, draft)
          ? askFor(step.name, draft)
          : `${labelFor(step.name, draft)} — this writes to ${step.app}.`}
      </p>
      {/* Approving a write without seeing what it will do is not a gate, so the
          body shows here even though the step list is otherwise quiet. */}
      {asking ? null : fields.length > 0 ? (
        <div className="agent__fields">
          {fields.map((field) => (
            <label className="agent__field" key={field.key}>
              <span className="agent__field-label">{field.label}</span>
              <input
                className="ask__input"
                value={field.get(draft)}
                placeholder={field.placeholder}
                // The first empty field is the one the agent could not fill.
                autoFocus={!field.get(draft)}
                onChange={(e) =>
                  setDraft((current) => field.set(current, e.target.value))
                }
              />
            </label>
          ))}
        </div>
      ) : (
        <div className="agent__body">{bodyFor(step.name, draft)}</div>
      )}
      <div className="agent__actions">
        <button className="agent__action" onClick={() => confirm(false)}>
          {asking ? "No thanks" : "Cancel"}
        </button>
        {asking ? (
          <button
            className="agent__action agent__action--go"
            autoFocus
            onClick={() => setFilling(true)}
          >
            Save contact
          </button>
        ) : (
          <button
            className="agent__action agent__action--go"
            autoFocus={fields.length === 0}
            onClick={() => confirm(true)}
          >
            Confirm
          </button>
        )}
      </div>
    </div>
  );
}

/** The flat string map the confirm command takes; see `display_args` in agent.rs. */
function asStrings(args: ToolArgs): Record<string, string> {
  return Object.fromEntries(
    Object.entries(args).map(([key, value]) => [
      key,
      typeof value === "string" ? value : JSON.stringify(value),
    ]),
  );
}

function StepIcon({ step, pending }: { step: Step; pending: boolean }) {
  if (step.result) {
    return step.result.success ? (
      <CheckIcon className="agent__icon" size={14} />
    ) : (
      <XIcon className="agent__icon agent__icon--bad" size={14} />
    );
  }
  if (step.confirmed === false) return <XIcon className="agent__icon" size={14} />;
  if (pending)
    return <TriangleAlertIcon className="agent__icon agent__icon--warn" size={14} />;
  return <LoaderCircleIcon className="agent__icon animate-spin" size={14} />;
}

/**
 * What a step's row says, as one string. Two adjacent steps sharing it render
 * identically, which is exactly when collapsing them hides nothing.
 */
function rowKey(step: Step, pending: boolean): string {
  return `${labelFor(step.name, step.args)}|${stepStatus(step, pending)}`;
}

function stepStatus(step: Step, pending: boolean): string {
  if (step.result) return step.result.success ? "done" : "failed";
  if (step.confirmed === false) return "cancelled";
  if (pending) return "needs approval";
  return "running…";
}

/**
 * The chrome both Ask and Agent draw inside the notch panel: what was heard,
 * a rule, the scrolling body, and a footer.
 */
function Panel({
  title,
  bodyRef,
  /** Omit to hide the copy button — there is nothing worth copying yet. */
  copyText,
  onClose,
  onAddTab,
  footer,
  children,
}: {
  title: string;
  bodyRef?: React.RefObject<HTMLDivElement | null>;
  copyText?: string;
  onClose: () => void;
  /** Omitted at MAX_TABS — the button goes away rather than sitting dead. */
  onAddTab?: () => void;
  /** Sits left of new/copy/close. */
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
        {/* The second way to open a tab. The strip's own `+` is the discoverable
            one; this sits in the row the eye already goes to for copy/close.
            Absent, not disabled, at the cap — matching the strip's. */}
        {onAddTab && (
          <button
            className="ask__button"
            aria-label="New tab"
            title="New tab"
            onClick={onAddTab}
          >
            <PlusIcon size={15} strokeWidth={2.5} />
          </button>
        )}
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
 * Cross-fades its contents whenever the phase changes. The `key` is the whole
 * mechanism: React remounts, so the CSS entrance in overlay.css restarts
 * without the usual reflow dance. Cheap — the subtree is a dot and a word.
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
  // CSS owns the motion; these two properties only give each bar its place in
  // the row and its height ceiling.
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
