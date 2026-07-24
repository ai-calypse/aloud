import React, {
  useCallback,
  useEffect,
  useMemo,
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
import {
  commands,
  events,
  type AgentPermission,
  type AgentSession,
  type Agent as CodingAgent,
  type ApprovalReply,
  type LocalRepo,
  type OverlayStyle,
  type Phase,
} from "./lib/bindings";
import { MessageResponse } from "@/components/ai-elements/message";
import { Shimmer } from "@/components/ai-elements/shimmer";
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { ChevronsUpDownIcon } from "lucide-react";
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
import { diffLines, diffStat } from "./lib/diff";
import {
  agentStage,
  agentHue,
  stageCat,
  stageSeed,
  syncStages,
  STAGE_LABEL,
  type AgentStage,
} from "./lib/agent-stage";
import { notchPath } from "./lib/notch-shape";
import { timeAgo } from "./lib/format";
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
 * The user's prompt can be a whole paragraph; capped so it never fills the
 * notch. The full text is on the `<p>`'s title, and the detail view a click
 * opens is where the rest lives.
 */
const PROMPT_CHARS = 140;

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
  /**
   * Whether the panel currently has content worth measuring. Part of the
   * rebuild key: the agents surface and the hover peek appear without a
   * dictation ever bumping `run`, and a driver built against an empty (or
   * `display: none`) body measured nothing — so the box never reached Rust
   * and every click on a visible permission card fell through to the app
   * behind the notch.
   */
  populated: boolean,
  /**
   * The resting strip is showing beside the cutout and owns the resting
   * click-box itself (it is wider than the cutout, and off-centre-left) so that
   * hovering it flips Rust's `hovering` and grows the notch. While that is true
   * the panel must not also write the box in its *unpopulated* branch, or the
   * two writers fight through the same report. Once hovering grows the notch
   * (populated), the panel reclaims the box — the strip bows out (`handedOff`).
   */
  ambientOwnsBox: boolean,
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
  // Bumped when the body mounts populated but has not laid out yet (offsetWidth
  // 0), so the build effect re-runs the instant it gets a size. Without it a
  // first paint with zero-width content bailed and never re-measured — the
  // notch grew on hover but reported only the cutout box, so Rust's poll judged
  // the cursor (now over a card) as outside and the hover never took until a
  // click forced a fresh cycle.
  const [layoutTick, setLayoutTick] = useState(0);

  // Rebuilt each time the panel appears, so every dictation starts its growth
  // from the cutout rather than from wherever the last one left the springs.
  useEffect(() => {
    const surfaceElement = surface.current;
    const shapeElement = shape.current;
    const pathElement = path.current;
    const contentElement = content.current;
    if (!surfaceElement || !shapeElement || !pathElement || !contentElement) return;
    // Nothing to show: at rest the surface *is* the cutout, nothing more.
    // Snap it there rather than leaving whatever inline size the last
    // driver's springs wrote — an empty panel keeping its old size is a
    // black slab hanging under the menu bar. The cutout box is still
    // reported, so hovering that strip is what grows the notch.
    //
    // Judged from `populated`, never from the body's measured width: a
    // `display: none` pill measures 0 while holding real content, and an
    // empty body measures its own padding — both lied here before.
    if (!populated) {
      const computed = getComputedStyle(surfaceElement);
      const measured = (name: string, fallback: number) => {
        const value = parseFloat(computed.getPropertyValue(name));
        return Number.isFinite(value) ? value : fallback;
      };
      const width = measured("--notch-w", NOTCH_FALLBACK.width);
      const height = measured("--notch-h", NOTCH_FALLBACK.height);
      surfaceElement.style.width = `${width}px`;
      surfaceElement.style.height = `${height}px`;
      shapeElement.setAttribute("viewBox", `0 0 ${width} ${height}`);
      pathElement.setAttribute(
        "d",
        notchPath(width, height, Math.max(0, height / 3 - 4), height / 3),
      );
      // When the ambient strip is up it owns the click-box (it is wider than
      // the cutout and reaches left of it); reporting the bare cutout here
      // would clobber that and the strip would stop taking clicks.
      if (!ambientOwnsBox) onTarget(width, height);
      return;
    }
    // On screen but not laid out yet (just mounted, or `display: none`):
    // nothing to measure. Watch for the layout and bump `layoutTick` to re-run
    // this effect the instant it has a size — `populated`/`run` may not change
    // again (a first hover flips `populated` once), so relying on them to come
    // back around stranded the notch reporting only the cutout box.
    if (!contentElement.offsetWidth) {
      const pending = new ResizeObserver(() => {
        if (contentElement.offsetWidth) setLayoutTick((n) => n + 1);
      });
      pending.observe(contentElement);
      return () => pending.disconnect();
    }

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
  }, [style, run, populated, ambientOwnsBox, layoutTick]);

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
 * The agents surface's slot in the strip — a fixed pseudo-tab, not an entry in
 * `tabs`: it cannot be closed, survives every run reset, and is what makes
 * "everything under one notch" true. Negative so no real tab id collides.
 */
const AGENTS_TAB = -1;

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
  // The notch opened by clicking it rather than by speaking. Its own state
  // because it belongs to no dictation and must survive `run` changing.
  const [browsing, setBrowsing] = useState(false);
  // Tool calls awaiting Approve / Deny, newest first — a swarm can ask several
  // at once, so this is a queue, not a single card. Keyed by tool_use_id.
  const [permissions, setPermissions] = useState<AgentPermission[]>([]);
  // The agent row the user drilled into, by session id, or null for the list.
  const [selected, setSelected] = useState<string | null>(null);
  // The enlarged cat preview: the stage + session it belongs to (from which the
  // image and caption derive) and the on-screen box to grow it from. Rendered
  // fixed at the root so it escapes the list's scroll clip; a 48px avatar hover
  // cannot show the meme, and the joke is the point.
  const [peek, setPeek] = useState<{
    stage: AgentStage;
    seed: string;
    rect: DOMRect;
  } | null>(null);
  const showPeek = useCallback(
    (stage: AgentStage | null, seed: string, anchor: HTMLElement | null) => {
      setPeek(stage && anchor ? { stage, seed, rect: anchor.getBoundingClientRect() } : null);
    },
    [],
  );
  // Any of them owns the panel; almost everything below only cares that one
  // does. A pending permission counts: the card has to be on screen to answer.
  // A pending permission is deliberately NOT part of this: a card *arriving*
  // opens the panel (the event listener sets `browsing`), but it must not
  // *hold* it — deriving openness from the queue pinned the panel so hard
  // that the cross, click-away and Escape all read as broken. Closing with a
  // card pending is fine: the hook waits (24h), the card stays queued under
  // the Agents tab's dot, and reopening the notch finds it again.
  const panelOpen = ask !== null || agent !== null || browsing;
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
  // The agents currently running, pushed from Rust. Independent of any
  // dictation: the whole point of the resting notch is that these are visible
  // when nothing is being said.
  const [sessions, setSessions] = useState<AgentSession[]>([]);
  // Pointer over the notch, from Rust's cursor poll — a click-through window
  // cannot see the mouse, so there is no `mouseenter` to use here.
  const [hovering, setHovering] = useState(false);
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

  // With no run to show, the agents tab is the only content the panel has —
  // opening by click, by a permission arriving, or by the run closing out
  // from under the strip all land there rather than on an empty tab 0.
  useEffect(() => {
    if (panelOpen && !ask && !agent) setActiveTab(AGENTS_TAB);
  }, [panelOpen, ask, agent]);

  // Dev/QA: `?agents=2` fakes a session list and `?hover` holds the peek row
  // open, so the resting notch's two states are reachable in a browser. The
  // real ones only arrive from Rust, and the hover only from a cursor poll.
  useEffect(() => {
    const params = new URLSearchParams(location.search);
    const count = Number(params.get("agents"));
    if (count > 0) {
      const ago = (m: number) => new Date(Date.now() - m * 60_000).toISOString();
      setSessions(
        Array.from({ length: count }, (_, i) => ({
          id: `preview:${i}`,
          agent: i % 2 ? "codex" : "claude",
          title:
            i === 0
              ? "yo no, the gif in the readme sucks. please fix"
              : `hello world. write ${i + 4} haikus`,
          prompt:
            i === 0
              ? "yo no, the gif in the readme sucks. please fix"
              : `hello world. write ${i + 4} haikus`,
          cwd: "/Users/y/dev/aloud-tauri",
          branch: i === 0 ? "feat/agent-notch" : null,
          tool: i === 0 ? "Bash: cargo test --workspace" : null,
          model: i % 2 ? "gpt-5.6-sol" : "fable-5",
          lastActivity: ago(i === 0 ? 0 : i * 10),
          subagents:
            i === 0
              ? [
                  { label: "Map the readme assets", model: "sonnet-5", tool: null, busy: true },
                  { label: "Regenerate the gif", model: "fable-5", tool: null, busy: false },
                ]
              : [],
          busy: i === 0,
          transient: null,
        })),
      );
    }
    if (params.has("hover")) setHovering(true);
    // `?perm` and `?perm=2` stage the card and a queue behind it.
    const perms = params.has("perm") ? Math.max(1, Number(params.get("perm")) || 1) : 0;
    if (perms > 0) {
      setPermissions(
        Array.from({ length: perms }, (_, i) => ({
          id: `preview:${i}`,
          // Ties the card to the first preview agent, so `?agents=2&perm` shows
          // the card under that row.
          sessionId: "preview:0",
          agent: "claude",
          tool: "Bash",
          input: { command: "rm -rf build/ && cargo clean", timeout: "120000" },
          cwd: "/Users/y/dev/aloud-tauri",
          questions: [],
          open: true,
        })),
      );
      // Mirrors what a real arrival does — open and drill.
      setBrowsing(true);
      setSelected("preview:0");
    }
    // `?edit` stages an Edit card — the diff view.
    if (params.has("edit")) {
      setPermissions([
        {
          id: "preview:e",
          sessionId: "preview:0",
          agent: "claude",
          tool: "Edit",
          input: {
            file_path: "/Users/y/dev/aloud-tauri/README.md",
            old_string: "## Why Aloud?\n\nHold a key, speak, and clean text lands\nin whatever app has focus.",
            new_string:
              "## Why Aloud?\n\nHold a key, speak, and clean text lands\nin whatever app has focus.\nAgents live in the notch now, too.",
            replace_all: "false",
          },
          cwd: "/Users/y/dev/aloud-tauri",
          open: true,
          questions: [],
        },
      ]);
      setBrowsing(true);
      setSelected("preview:0");
    }
    // `?question` stages an AskUserQuestion card — the multi-option prompt.
    if (params.has("question")) {
      setPermissions([
        {
          id: "preview:q",
          sessionId: "preview:0",
          agent: "claude",
          tool: "AskUserQuestion",
          input: {},
          cwd: "/Users/y/dev/aloud-tauri",
          open: true,
          questions: [
            {
              question: "Which auth method should the API use?",
              header: "Auth method",
              multiSelect: false,
              options: [
                { label: "OAuth", description: "Redirect flow, refresh tokens" },
                { label: "API keys", description: "Simple, per-service secrets" },
              ],
            },
            {
              question: "Which environments should this ship to?",
              header: "Environments",
              multiSelect: true,
              options: [
                { label: "Staging", description: "" },
                { label: "Production", description: "Behind the canary" },
              ],
            },
          ],
        },
      ]);
      setBrowsing(true);
      setSelected("preview:0");
    }
  }, []);

  // Agents, hover and permission requests, all independent of any dictation.
  useEffect(() => {
    const sessionsOff = events.agentSessions.listen((e) => setSessions(e.payload));
    const hoverOff = events.notchHover.listen((e) => setHovering(e.payload.over));
    // Newest on top, and de-duplicated by id: a hook that fires twice for one
    // call must not stack two cards for it. Arrival is the one-shot that
    // opens the panel and drills to the asking agent — an *event*, not a
    // derived state, so the user can navigate away and close afterwards.
    const permOff = events.agentPermission.listen((e) => {
      setPermissions((queue) => [e.payload, ...queue.filter((p) => p.id !== e.payload.id)]);
      // `open` is the "open notch for permissions" setting, decided in Rust
      // so this and the window agree. Off: the card queues behind the hover.
      if (e.payload.open) {
        setBrowsing(true);
        setSelected(e.payload.sessionId ?? e.payload.id);
      }
    });
    // Answered here, abandoned by the agent, or the app deciding for us: the
    // card comes down the same way in every case.
    const resolvedOff = events.agentPermissionResolved.listen((e) =>
      setPermissions((queue) => queue.filter((p) => p.id !== e.payload.id)),
    );
    return () => {
      void sessionsOff.then((off) => off());
      void hoverOff.then((off) => off());
      void permOff.then((off) => off());
      void resolvedOff.then((off) => off());
    };
  }, []);

  // Answer, then drop it locally — the resolved event will also drop it, and
  // whichever lands first wins. Removing it here keeps the next card from
  // waiting on the round-trip.
  const resolvePermission = useCallback((id: string, reply: ApprovalReply) => {
    void commands.resolvePermission(id, reply);
    setPermissions((queue) => queue.filter((p) => p.id !== id));
  }, []);

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
  const closeTab = useCallback(
    (id: number) => {
      setTabs((current) =>
        current.map((tab) => (tab.id === id ? { ...tab, closing: true } : tab)),
      );
      // Closing the active tab lands on the run when there is one, else on
      // the agents — tab 0 without a run is an empty panel.
      const home = ask || agent ? 0 : AGENTS_TAB;
      setActiveTab((active) => (active === id ? home : active));
    },
    [ask, agent],
  );

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

  /**
   * Nothing is being said and no panel is up: the notch is just sitting there.
   * This is the state that did not exist before — the window used to be hidden
   * entirely between dictations.
   */
  const resting = !panelOpen && phase === "idle" && !dismissing;
  const busyCount = sessions.filter((s) => s.busy).length;
  // The ambient sign: what the resting notch shows *without* a hover — a small
  // strip of stage cats left of the camera while agents are busy, or an
  // attention pulse when one wants permission and the notch did not auto-open
  // (that setting off; when on, the panel is up and we are not resting).
  // Hovering it expands the stack into a labelled horizontal row and offers a
  // "view all agents" button; it owns the resting click-box while up.
  const ambient = resting && (busyCount > 0 || permissions.length > 0);
  // Drill into one agent (opens the panel on its detail) or open the whole
  // list — the two things the expanded ambient row can do. Both route through
  // the same open-the-notch state a click does today.
  const openAgent = useCallback((id: string) => {
    setBrowsing(true);
    setSelected(id);
  }, []);
  const openAgents = useCallback(() => {
    setBrowsing(true);
    setSelected(null);
  }, []);

  // The drill is plain navigation state. A permission arriving *sets* it
  // (one-shot, in the event listener) — deriving it from the queue every
  // render meant "back" un-drilled and the fallback instantly re-drilled,
  // which read as the button not working.
  const selectedSession = sessions.find((s) => s.id === selected) ?? null;

  // Reconcile the cat-variation counters once for the whole list before any
  // avatar reads a seed: bumps a session's counter on stage change and prunes
  // sessions that are gone, so the map stays bounded. Memoised on the sessions'
  // (id, stage) signature — the only inputs that move the counters — so it runs
  // on a real change, not on every streamed token. `asking` here mirrors the
  // per-row derivation: a pending permission is `waiting_input`.
  const askingNow = new Set(permissions.map((p) => p.sessionId).filter(Boolean));
  const stageSig = sessions
    .map((s) => `${s.id}:${agentStage(s, askingNow.has(s.id))}`)
    .join("|");
  useMemo(() => {
    syncStages(
      sessions.map((s) => ({ id: s.id, stage: agentStage(s, askingNow.has(s.id)) })),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stageSig]);

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
  // Whether the notch has anything to show: a panel, a dictation in some
  // phase, or the hover peek. This is the state the driver sizes from —
  // judging by the body's measured width instead was wrong twice (a
  // `display: none` pill measures 0 with content, an empty body measures its
  // padding without any), so the decision is state, never pixels.
  const populated = panelOpen || phase !== "idle" || (resting && hovering);
  // The strip owns the click-box only while it is the thing on screen — i.e. at
  // rest, NOT once a hover has grown the notch. Passing bare `ambient` here left
  // the strip's small resting box reported after the panel grew, so the cursor
  // (now over a dealt card well below the old cutout) tested as OUTSIDE and the
  // window went click-through: the card never got `mouseenter`, so the hover
  // pop/plate did not fire until a click→back forced the grown box to report.
  const panel = useNotchPanel(
    effectiveStyle,
    run,
    populated,
    ambient && !hovering,
    dismissing,
    reportBox,
  );

  // Tells Rust whether a panel holds the window's focus. This is the *focus*
  // signal, not the click signal: clicks are gated entirely by Rust's cursor
  // poll against the panel rect (in every regime), so an open panel never turns
  // the whole window into a click trap. This just lets the panel take focus for
  // text inputs and lets an outside-click dismiss it (focus-loss in Rust).
  useEffect(() => {
    try {
      void commands.setPanelInteractive(panelOpen);
    } catch {
      /* browser preview — nothing is tracking the pointer either */
    }
  }, [panelOpen]);
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
        // A dictation started over the top of an answer takes the panel back —
        // including from the agents surface, or the pill and the panel fight
        // over the same strip.
        setAsk(null);
        setAgent(null);
        setBrowsing(false);
        setSelected(null);
        setPartial("");
        setDroppedDown(false);
        setRun((previous) => previous + 1);
      }
    });
    // A dismiss is Rust declaring the panel over, so the panel *state* goes
    // with it. Leaving `ask`/`agent` latched here is what made the panel
    // spring back open the moment `dismissing` reset — and the second click
    // on the cross was then a no-op, because Rust's `end_panel` had already
    // cleared its side and had nothing left to close.
    const unlistenDismiss = events.overlayDismiss.listen(() => {
      setDismissing(true);
      setAsk(null);
      setAgent(null);
      setBrowsing(false);
      setSelected(null);
    });
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
      if (e.key !== "Escape") return;
      // A browse sets `PANEL_OPEN` in Rust (via `setPanelInteractive`), so
      // closing it has to clear that too — `closeBrowse` does, and clearing
      // only the webview `browsing` would leave the Rust flag stuck.
      if (!agent && !ask) {
        void commands.closeBrowse();
        setBrowsing(false);
        setSelected(null);
        return;
      }
      // `closeAgent` also abandons a pending confirmation, so Escape reads as
      // "no" rather than leaving a write waiting on a channel nobody answers.
      void (agent ? commands.closeAgent() : commands.closeAsk());
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [panelOpen, agent, ask]);

  // A dismissal is an animation, not a state of being: once the collapse has
  // settled, the panel is back to resting so the notch (and the agents under
  // it) are reachable again. Without this, Escape after an ask left
  // `dismissing` true forever — a live window rendering nothing, hover dead.
  //
  // The phase resets here too, and only here. The `idle` that ended the run
  // is deliberately never stored while the exit plays (blanking mid-collapse
  // cut a no-speech run off dead), so the last real phase is still latched —
  // and a resting notch that re-opens onto a stale "Transcribing…" pill is
  // that latch showing. Collapse settled = the run is over = idle.
  useEffect(() => {
    if (!dismissing) return;
    const timer = setTimeout(() => {
      setDismissing(false);
      setPhase("idle");
    }, 600);
    return () => clearTimeout(timer);
  }, [dismissing]);

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
    <div
      className="overlay-root"
      ref={root}
      data-exiting={dismissing || undefined}
      // Belt-and-suspenders close. The primary popover dismiss is now
      // focus-loss in Rust (`dismiss_on_focus_loss`) — a click outside the
      // panel rect reaches the app behind *and* resigns the overlay's key
      // focus. This only fires for a click that lands inside the window's own
      // clickable rect but outside the pill (e.g. a rect edge), where no focus
      // change happens.
      onMouseDown={(e) => {
        if (!panelOpen) return;
        if ((e.target as HTMLElement).closest(".drop-pill")) return;
        if (agent) void commands.closeAgent();
        else if (ask) void commands.closeAsk();
        else {
          void commands.closeBrowse();
          setBrowsing(false);
          setSelected(null);
        }
      }}
    >
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
        className={`drop-pill drop-pill--${panelOpen ? "answering" : resting ? "resting" : phase}`}
        data-hover={resting && hovering ? "" : undefined}
        ref={panel.surface}
        // Only ever reaches the DOM while the pointer is over the panel — Rust
        // hands the window clicks exactly then (`track_cursor`).
        onClick={() => {
          if (resting && sessions.length > 0) setBrowsing(true);
        }}
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
              : resting && hovering
                ? " drop-pill__body--agents"
                : transcript
                  ? " drop-pill__body--transcript"
                  : ""
          }`}
          ref={panel.content}
        >
          {resting ? (
            // At rest the panel is exactly the cutout and this body is empty;
            // the cats show on the strip left of the notch (`AmbientSign`).
            // Hovering grows the notch down (the spring in `useNotchPanel`) and
            // deals the same agents out into it as a row of bigger cards
            // (`NotchAgents`), while the strip fades. This is the "agents
            // animate inside the notch on hover" the notch is for.
            hovering && (
              <NotchAgents
                sessions={sessions}
                permissions={permissions}
                onOpenAgent={openAgent}
                onOpenAgents={openAgents}
                onResolve={resolvePermission}
              />
            )
          ) : panelOpen ? (
            // Everything under one notch: the strip always shows, the agents
            // are a tab on it, and a dictation's ask/agent panel is another —
            // so escaping one never takes the other away.
            <>
              <TabStrip
                tabs={tabs}
                active={activeTab}
                runOpen={ask !== null || agent !== null}
                asking={permissions.length > 0}
                title={
                  ask?.question ??
                  agent?.heard ??
                  // Opened from the notch rather than by speaking, so there is
                  // no utterance to name the tab after.
                  "Aloud"
                }
                onSelect={setActiveTab}
                onClose={closeTab}
                onClosed={dropTab}
                onAdd={addTab}
              />
              {activeTab === AGENTS_TAB ? (
                // The agents surface: the list, or one agent's live detail,
                // with any pending permission under the row that is asking.
                selectedSession ? (
                  <AgentDetail
                    session={selectedSession}
                    permission={
                      permissions.find((p) => p.sessionId === selectedSession.id) ?? null
                    }
                    onResolve={resolvePermission}
                    onBack={() => setSelected(null)}
                    onPeek={showPeek}
                  />
                ) : (
                  <AgentList
                    sessions={sessions}
                    permissions={permissions}
                    onSelect={setSelected}
                    onResolve={resolvePermission}
                    onPeek={showPeek}
                    onClose={() => {
                      setBrowsing(false);
                      setSelected(null);
                      // With a run open the X leaves the panel up on that run;
                      // only a browse-only panel actually closes — and that is
                      // the case that has to clear Rust's `PANEL_OPEN`.
                      if (ask || agent) setActiveTab(0);
                      else void commands.closeBrowse();
                    }}
                  />
                )
              ) : /* Keyed so switching back to a tab replays its entrance rather
                  than having the content appear already settled. */
              tabs.find((tab) => tab.id === activeTab)?.blank ? (
                // A new tab is where you start a coding agent. Keyed on the tab
                // so switching back replays its entrance, like the run tabs.
                <CodingAgentComposer
                  key={activeTab}
                  bodyRef={answerRef}
                  onClose={() => closeTab(activeTab)}
                  onAddTab={tabs.length < MAX_TABS ? addTab : undefined}
                />
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
    {/* The enlarged cat, fixed at the root so it escapes the agent list's
        scroll clip. Grows out of the hovered avatar's box (its origin), which
        is why it is positioned from the anchor rect rather than centred. */}
    {peek && <CatPeek stage={peek.stage} seed={peek.seed} rect={peek.rect} />}
    {/* The ambient sign lives on the canvas beside the cutout, not in the pill —
        agent-notch's trick: a full transparent top-bar canvas lets a mascot be
        drawn left of the notch without growing any box. Click-through
        (`pointer-events: none`), so it never touches the hit-test region. */}
    {ambient && (
      <AmbientSign
        sessions={sessions}
        permissions={permissions}
        // Hovering grew the notch and the dealt row took over: fade the strip
        // out (it is the same agents, now shown bigger inside the notch).
        handedOff={hovering}
        onBox={reportBox}
      />
    )}
    </div>
  );
}

/**
 * The blown-up meme on avatar hover. Anchored to the avatar's on-screen box and
 * grown from there — the small round cat loses the joke, this gives it back.
 * CSS owns the grow (the target never changes mid-flight), so it is a plain
 * mount + a class the next frame flips.
 */
function CatPeek({ stage, seed, rect }: { stage: AgentStage; seed: string; rect: DOMRect }) {
  const [shown, setShown] = useState(false);
  // A frame after mount, so the transition has a from-state to leave.
  useEffect(() => {
    const id = requestAnimationFrame(() => setShown(true));
    return () => cancelAnimationFrame(id);
  }, []);
  const SIZE = 180;
  // Grow up-and-right out of the avatar, clamped to the viewport so it never
  // clips off the top or right edge of the overlay canvas.
  const left = Math.min(rect.left, window.innerWidth - SIZE - 8);
  const top = Math.max(8, rect.bottom - SIZE);
  return (
    <div
      className="cat-peek"
      data-shown={shown || undefined}
      aria-hidden
      style={{
        left: `${left}px`,
        top: `${top}px`,
        width: `${SIZE}px`,
        // Origin = where the avatar sits relative to the peek's top-left, so it
        // scales out of the cat rather than out of its own centre.
        transformOrigin: `${rect.left - left + rect.width / 2}px ${rect.top - top + rect.height / 2}px`,
      }}
    >
      <img className="cat-peek__img" src={stageCat(stage, seed)} alt="" />
      <span className="cat-peek__cap">{STAGE_LABEL[stage]}</span>
    </div>
  );
}

/** How many stage cats the resting strip shows before collapsing to a `+N`. */
const AMBIENT_CATS = 4;

/**
 * The resting strip: a tight stack of stage cats on the canvas just *left* of
 * the camera (agent-notch's approach, not a growth of the drop-pill), shown at
 * rest without a hover. It does NOT expand in place — hovering hands off to the
 * notch, which grows down and *deals* the same cats out into a row of bigger
 * circles (`NotchAgents`, dealing from the strip's side via `--from`). The
 * strip just fades out once the notch takes over.
 *
 * It still reports the resting click-box, though — the window only takes the
 * mouse over a reported box, and the strip sits left of the cutout, so without
 * this the hover that grows the notch would never fire when the cursor is over
 * the strip. Symmetric across the canvas (see `point_in_panel`): a width that
 * reaches the strip on the left also reaches its mirror on the right, with the
 * centred notch inside it either way — harmless dead canvas.
 */
function AmbientSign({
  sessions,
  permissions,
  handedOff,
  onBox,
}: {
  sessions: AgentSession[];
  permissions: AgentPermission[];
  /** The notch has grown and shows the dealt row now — fade the strip out. */
  handedOff: boolean;
  onBox: (width: number, height: number) => void;
}) {
  const el = useRef<HTMLDivElement>(null);
  const asking = permissions.length > 0;
  const askingIds = new Set(permissions.map((p) => p.sessionId).filter(Boolean));
  // Busy-first from Rust; the busy ones are what the sign is about. Asking but
  // nothing busy (a parked agent) still needs a face, so fall back to it.
  const busy = sessions.filter((s) => s.busy);
  const cats = ambientCats(sessions);
  const extra = (busy.length > 0 ? busy.length : sessions.length) - cats.length;

  // Report the strip's rect as the resting click-box, so hovering the strip (or
  // the notch) makes Rust's poll flip `hovering` — which is what grows the
  // notch. A ResizeObserver keeps it current if the cat count changes. Once
  // handed off, the grown notch owns the box (`useNotchPanel` reports it): the
  // strip bows out so the two do not fight over the shared report each frame.
  useEffect(() => {
    if (handedOff) return;
    const node = el.current;
    const root = node?.offsetParent as HTMLElement | null;
    if (!node || !root) return;
    const report = () => {
      const rect = node.getBoundingClientRect();
      const rootRect = root.getBoundingClientRect();
      const centre = rootRect.left + rootRect.width / 2;
      const halfWidth = centre - rect.left;
      onBox(halfWidth * 2, rect.height);
    };
    report();
    const observer = new ResizeObserver(report);
    observer.observe(node);
    return () => observer.disconnect();
  }, [onBox, handedOff]);

  return (
    <div
      ref={el}
      className="ambient"
      data-handed-off={handedOff || undefined}
      data-asking={asking || undefined}
      data-busy={busy.length > 0 || undefined}
    >
      <div className="ambient__cats">
        {cats.map((session) => {
          const stage = agentStage(session, askingIds.has(session.id));
          return (
            <img
              key={session.id}
              className="ambient__cat-img"
              src={stageCat(stage, stageSeed(session.id, stage))}
              alt=""
              aria-hidden
              data-asking={askingIds.has(session.id) || undefined}
            />
          );
        })}
        {extra > 0 && <span className="ambient__count">+{extra}</span>}
      </div>
    </div>
  );
}

/**
 * The cats the resting strip (and, one-to-one, the dealt row) show: the busy
 * agents, or — if none are busy but one is asking — that one, so the sign has a
 * face to ring. Capped at `AMBIENT_CATS`. Shared so the strip and the row agree
 * on exactly which cats exist.
 */
function ambientCats(sessions: AgentSession[]): AgentSession[] {
  const busy = sessions.filter((s) => s.busy);
  return (busy.length > 0 ? busy : sessions).slice(0, AMBIENT_CATS);
}

/**
 * Deal tuning, chosen on the mockup at
 * claude.ai/code/artifact/e0d1601a-a6e3-453e-8ee2-f64e172e3244 (layout B).
 * `SPACING` is the gap between dealt circles' centres; `STAGGER` the beat
 * between each card being flicked out — small, so the row deals as a quick
 * ripple rather than one-at-a-time. `520ms` is the per-card travel, set in CSS.
 */
const DEAL_SPACING = 92;
const DEAL_STAGGER = 40;
/** The dealt card's width — matches `.notch-card` in overlay.css. Sizes the
 *  deal layer so the panel measures the full row (absolute cards do not). */
const DEAL_CARD_W = 86;
/** How long the cursor must rest on one card before the plate unfolds the full
 *  detail. Long enough that a sweep across the row never triggers it. */
const DWELL_MS = 500;

/**
 * The agents dealt into the notch, shown while the resting notch is hovered.
 * The same cats the resting strip shows, but bigger and circular — they deal
 * out from the strip's stacked position into a straight row (layout B), each
 * flicked a beat after the last, like a dealer going round the table. One
 * card per agent: the cat, its status underneath. Hovering a card lifts it;
 * clicking opens that agent's detail. "View all agents" opens the full list.
 *
 * The deal is pure CSS: each card starts collapsed on the resting stack
 * (`--from`, left of the cutout) and transitions to `translateX(--tx)` with a
 * per-card `transition-delay`. Mount plays it once (the notch only mounts this
 * on hover), armed by flipping `data-dealt` a frame after mount so the cards
 * have a from-state to leave.
 */
function NotchAgents({
  sessions,
  permissions,
  onOpenAgent,
  onOpenAgents,
  onResolve,
}: {
  sessions: AgentSession[];
  permissions: AgentPermission[];
  onOpenAgent: (id: string) => void;
  onOpenAgents: () => void;
  onResolve: (id: string, reply: ApprovalReply) => void;
}) {
  const askingIds = new Set(permissions.map((p) => p.sessionId).filter(Boolean));
  const cats = ambientCats(sessions);
  // Deal on mount: `dealt` is false for the first paint (cards sit on the
  // stack), true the next frame (they deal out). A ref-free flag driven by one
  // rAF — no per-frame state, just the single flip the transition needs.
  const [dealt, setDealt] = useState(false);
  // Which card the cursor is on, or null. Drives the shared detail plate below
  // the row — one plate, not one per card, so gliding across just re-keys its
  // contents rather than opening and closing three separate surfaces.
  const [hovered, setHovered] = useState<string | null>(null);
  // Progressive reveal: a quick hover shows the compact plate (+ live "Now"
  // line); *dwelling* on one card past `DWELL_MS` unfolds the full detail
  // (cwd, prompt, subagents). Sweeping the row peeks; lingering commits to
  // reading — without a click. Reset whenever the hovered card changes.
  const [dwelling, setDwelling] = useState(false);
  useEffect(() => {
    const id = requestAnimationFrame(() => setDealt(true));
    return () => cancelAnimationFrame(id);
  }, []);
  useEffect(() => {
    setDwelling(false);
    if (!hovered) return;
    const t = setTimeout(() => setDwelling(true), DWELL_MS);
    return () => clearTimeout(t);
  }, [hovered]);
  if (cats.length === 0) {
    return <p className="notch-peek--empty">No agents running</p>;
  }
  const mid = (cats.length - 1) / 2;
  // The deal layer must be sized *explicitly*: its cards are absolutely
  // positioned, so they contribute nothing to the content box the drop-pill
  // spring measures — without this the panel collapses to the button's width
  // and the dealt row overflows a too-narrow notch. Width spans edge card to
  // edge card; height fits a card + its label.
  const dealWidth = (cats.length - 1) * DEAL_SPACING + DEAL_CARD_W;
  // Only scan when something is actually hovered; the height is a constant the
  // CSS owns, so only the derivable width is set inline here.
  const hoveredSession = hovered ? (cats.find((s) => s.id === hovered) ?? null) : null;
  return (
    <div className="notch-agents" data-dealt={dealt || undefined}>
      <div className="notch-deal" style={{ width: dealWidth }}>
        {cats.map((session, i) => (
          <NotchCard
            key={session.id}
            session={session}
            asking={askingIds.has(session.id)}
            tx={(i - mid) * DEAL_SPACING}
            stagger={i * DEAL_STAGGER}
            onOpen={() => onOpenAgent(session.id)}
            // A leave clears only if this card still owns the hover — the
            // browser fires the next card's enter before this leave, so a bare
            // `null` would wipe the freshly-hovered card.
            onHover={(on) =>
              setHovered((h) => (on ? session.id : h === session.id ? null : h))
            }
          />
        ))}
      </div>
      {/* The peek under the row for the hovered card: a compact head on a quick
          hover, and — once dwelt on past DWELL_MS — the *same* full detail view
          a click opens (`AgentDetailBody`), so you can read what an agent is
          doing without clicking in. One shared surface; gliding re-keys it. */}
      <NotchPeekPlate
        session={hoveredSession}
        permissions={permissions}
        dwelling={dwelling}
        onResolve={onResolve}
      />
      <button className="notch-agents__all" type="button" onClick={onOpenAgents}>
        View all agents
      </button>
    </div>
  );
}

/**
 * The hover pop's spring. Matches the app's pop character (motion.css
 * `--spring-pop`, damping 0.62 → ~8% overshoot — the little rebound that reads
 * as landing), on a snappy response because it fires every time the cursor
 * crosses a card. `response` is seconds, per `lib/spring.ts`.
 */
const POP_DAMPING = 0.62;
const POP_RESPONSE = 0.28;

/**
 * One dealt card: the meme cat (its *stage*), inside a per-agent colour ring
 * with a stage-state badge in the corner, and `project @ branch` underneath —
 * so which agent it is, and which one you are about to click, reads before any
 * hover. The cat alone could not carry that: it is keyed to the stage, which is
 * transient and identical across two agents doing the same thing.
 *
 * The card also has a *live* hover pop. The pop can be redirected mid-flight —
 * glide from one cat to the next and the first retracts while the second grows,
 * reversible halfway without stalling — which is exactly what `lib/spring.ts`
 * is for (a baked CSS curve retargeted halfway restarts from zero velocity; the
 * live spring carries its momentum through). A spring drives a `--pop` value
 * 0→1 written straight onto the node each frame (no React state between
 * frames); CSS turns it into a scale+lift on the profile image. The image, not
 * the card: the card's transform is owned by the deal, so layering the pop on
 * the image keeps the two from fighting.
 */
function NotchCard({
  session,
  asking,
  tx,
  stagger,
  onOpen,
  onHover,
}: {
  session: AgentSession;
  asking: boolean;
  tx: number;
  stagger: number;
  onOpen: () => void;
  /** Cursor entered (true) or left (false) this card — drives the shared plate. */
  onHover: (on: boolean) => void;
}) {
  const stage = agentStage(session, asking);
  const seed = stageSeed(session.id, stage);
  const state = rowState(session);
  const el = useRef<HTMLButtonElement>(null);
  const pop = useRef<ReturnType<typeof createSpring> | null>(null);
  useEffect(() => {
    const node = el.current;
    if (!node) return;
    const spring = createSpring((value) => node.style.setProperty("--pop", String(value)), {
      damping: POP_DAMPING,
      response: POP_RESPONSE,
    });
    pop.current = spring;
    return () => spring.stop();
  }, []);
  return (
    <button
      ref={el}
      className="notch-card"
      type="button"
      data-asking={asking || undefined}
      title={rowName(session)}
      // Only the genuinely per-card values live here: the dealt slot `--tx`, the
      // identity hue `--ring`, and the deal's stagger. `--from` (where it deals
      // from) is identical for every card, so it lives in the CSS rule.
      style={
        {
          "--tx": `${tx}px`,
          "--ring": `hsl(${agentHue(session.id)} 70% 58%)`,
          transitionDelay: `${stagger}ms`,
        } as React.CSSProperties
      }
      onMouseEnter={() => {
        pop.current?.to(1);
        onHover(true);
      }}
      onMouseLeave={() => {
        pop.current?.to(0);
        onHover(false);
      }}
      onClick={onOpen}
    >
      <span className="notch-card__disc">
        <img className="notch-card__cat" src={stageCat(stage, seed)} alt="" aria-hidden />
        {/* The stage badge: what it is doing right now, colour-coded the same
            way the agent-list rows are (busy/live/done, orange when asking). */}
        <span
          className="notch-card__badge"
          data-state={asking ? "asking" : state}
          aria-hidden
        />
      </span>
      {/* Identity, not stage: project @ branch, so the card says *which* agent. */}
      <span className="notch-card__label">{rowName(session)}</span>
    </button>
  );
}

/** The agent's display name — what `agent-notch` puts in bold at the row head. */
const AGENT_NAME: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex",
};

/**
 * The peek under the dealt row for the hovered card — read an agent without a
 * click, in two tiers:
 *
 * - A quick hover shows a compact head: `project @ branch`, the stage, the
 *   agent · model · subagent count, and the live "Now" tool.
 * - *Dwelling* past `DWELL_MS` swaps that for the **same full detail view a
 *   click opens** (`AgentDetailBody`: avatar, cwd, the "You:" prompt, the
 *   approve/deny card if it is asking, Now, and every subagent) — the identical
 *   component and width, so the peek is the real thing, just without the click.
 *   Clicking still opens the pinned version that survives mouse-leave.
 *
 * One shared surface for the whole row: gliding re-keys it. It holds the last
 * session through the collapse so the fold never blanks mid-way; and both tiers
 * are pinned to the detail's own width (a `min-width`), so the peek is always
 * the notch-panel size, never as wide as its longest line of text.
 */
function NotchPeekPlate({
  session,
  permissions,
  dwelling,
  onResolve,
}: {
  session: AgentSession | null;
  permissions: AgentPermission[];
  /** The cursor has rested on this card long enough to show the full detail. */
  dwelling: boolean;
  onResolve: (id: string, reply: ApprovalReply) => void;
}) {
  // Hold the last session through the collapse, so it does not blank out while
  // the plate is still folding away.
  const shown = useRef<AgentSession | null>(null);
  if (session) shown.current = session;
  const s = shown.current;
  const permission = s ? (permissions.find((p) => p.sessionId === s.id) ?? null) : null;
  const asking = permission !== null;
  const stage = s ? agentStage(s, asking) : "idle";
  const state = s ? rowState(s) : "done";
  const subs = s?.subagents.length ?? 0;
  // Full detail only once the cursor has settled on a card — never mid fold-away
  // (no session), or it would grow as it is trying to leave.
  const expanded = dwelling && session !== null;
  return (
    <div
      className="notch-plate"
      data-open={session ? "" : undefined}
      data-expanded={expanded ? "" : undefined}
      aria-hidden={!session}
    >
      {s &&
        (expanded ? (
          // The real detail view a click opens — no back button (nowhere to go),
          // and no cat-peek inside a peek.
          <div className="notch-plate__inner notch-plate__inner--full" key={`${s.id}-full`}>
            <AgentDetailBody
              session={s}
              permission={permission}
              onResolve={onResolve}
              onPeek={NO_PEEK}
            />
          </div>
        ) : (
          <div className="notch-plate__inner" key={s.id}>
            <span className="notch-plate__title">{rowName(s)}</span>
            {/* What it is doing right now, led by the badge's own status dot. */}
            <span className="notch-plate__stage">
              <span className="notch-plate__pip" data-state={asking ? "asking" : state} aria-hidden />
              {STAGE_LABEL[stage]}
            </span>
            <span className="notch-plate__meta">
              {AGENT_NAME[s.agent] ?? s.agent}
              {s.model && <span className="notch-plate__dot"> · {s.model}</span>}
              {subs > 0 && (
                <span className="notch-plate__dot">
                  {" "}
                  · {subs} subagent{subs === 1 ? "" : "s"}
                </span>
              )}
            </span>
            <span className="notch-plate__now">
              <span className="notch-plate__now-label">Now</span>
              <span className="notch-plate__now-tool" data-idle={!s.tool || undefined}>
                {s.tool ?? (s.busy ? "thinking…" : "idle")}
              </span>
            </span>
          </div>
        ))}
    </div>
  );
}

/** The hover peek shows the detail inline; the enlarged cat-peek does not fire
 *  inside it, so `AgentDetailBody`'s avatar gets a no-op peek. */
const NO_PEEK: PeekFn = () => {};

/** The input keys worth leading with, per tool — the headline before the rest. */
const HEADLINE_ARG: Record<string, string> = {
  Bash: "command",
  Read: "file_path",
  Write: "file_path",
  Edit: "file_path",
  Glob: "pattern",
  Grep: "pattern",
};

/**
 * The Approve / Deny card. The agent is blocked on this — the socket thread in
 * `sessions.rs` is parked until `onResolve` sends the answer back.
 *
 * The whole tool call is shown, not a summary: the headline argument first
 * (a command, a path), then the rest, then the cwd — the point of a gate is
 * that the user can see exactly what would run.
 */
/**
 * One pending request: the model asking a question gets the question card,
 * anything else gets the approval card. Both resolve through the same
 * command — a question's answers are just a richer reply.
 */
function PermissionCard({
  request,
  queued,
  onResolve,
}: {
  request: AgentPermission;
  queued: number;
  onResolve: (id: string, reply: ApprovalReply) => void;
}) {
  return request.questions.length > 0 ? (
    <QuestionCard request={request} queued={queued} onResolve={onResolve} />
  ) : (
    <ApprovalCard request={request} queued={queued} onResolve={onResolve} />
  );
}

/**
 * The permission prompt, with the terminal's own three options — the point is
 * that answering here reads exactly like answering in the terminal would.
 */
function ApprovalCard({
  request,
  queued,
  onResolve,
}: {
  request: AgentPermission;
  queued: number;
  onResolve: (id: string, reply: ApprovalReply) => void;
}) {
  const headlineKey = HEADLINE_ARG[request.tool];
  const headline = headlineKey ? request.input[headlineKey] : undefined;
  // An edit's before/after renders as a real diff, not two walls of text
  // under `old_string` / `new_string` keys. Neither reference app does this —
  // open-vibe-island decodes the strings and discards them — but the card
  // already carries them whole, so showing the actual change is nearly free.
  const change =
    request.tool === "Edit" && request.input.old_string !== undefined
      ? diffLines(request.input.old_string, request.input.new_string ?? "")
      : request.tool === "Write" && request.input.content !== undefined
        ? diffLines("", request.input.content)
        : null;
  const DIFFED = ["old_string", "new_string", "content"];
  const rest = Object.entries(request.input).filter(
    ([k]) => k !== headlineKey && (change === null || !DIFFED.includes(k)),
  );
  // The third option opens a message box instead of denying outright — the
  // "tell Claude what to do differently" the terminal offers.
  const [denying, setDenying] = useState(false);
  const [message, setMessage] = useState("");

  // Enter allows, Escape denies — the two things a hand on the keyboard
  // reaches for. Scoped to the card so it does not fight the panel's own
  // Escape. While the message box is open, Enter belongs to it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" && !denying) onResolve(request.id, { kind: "allowOnce" });
      if (e.key === "Escape") {
        e.stopPropagation();
        onResolve(request.id, { kind: "deny", message: null });
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [request.id, onResolve, denying]);

  return (
    <div className="perm">
      <div className="perm__head">
        <span className="perm__agent">{AGENT_NAME[request.agent] ?? request.agent}</span>
        <span className="perm__wants">wants to run</span>
        {queued > 0 && <span className="perm__queue">+{queued} more</span>}
      </div>

      <div className="perm__call">
        <span className="perm__tool">{request.tool}</span>
        {headline && <code className="perm__headline">{headline}</code>}
        {change && <span className="perm__stat">{diffStat(change)}</span>}
      </div>

      {change && (
        <div className="diff">
          {change.map((line, i) => (
            <div className="diff__line" data-kind={line.kind} key={i}>
              <span className="diff__sign" aria-hidden>
                {line.kind === "add" ? "+" : line.kind === "del" ? "−" : " "}
              </span>
              {/* `|| " "` keeps an empty line at line height. */}
              {line.text || " "}
            </div>
          ))}
        </div>
      )}

      {rest.length > 0 && (
        <dl className="perm__args">
          {rest.map(([key, value]) => (
            <div className="perm__arg" key={key}>
              <dt>{key}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      )}

      {request.cwd && <p className="perm__cwd">{request.cwd}</p>}

      <div className="perm__options">
        <button
          className="perm__option perm__option--yes"
          onClick={() => onResolve(request.id, { kind: "allowOnce" })}
        >
          Yes
        </button>
        {request.tool && (
          <button
            className="perm__option"
            onClick={() => onResolve(request.id, { kind: "allowAlways" })}
          >
            Yes, don't ask again for {request.tool} this session
          </button>
        )}
        {denying ? (
          <form
            className="perm__deny-form"
            onSubmit={(e) => {
              e.preventDefault();
              onResolve(request.id, { kind: "deny", message: message.trim() || null });
            }}
          >
            <input
              className="perm__deny-input"
              autoFocus
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              placeholder="Tell Claude what to do differently…"
            />
            <button className="perm__option perm__option--no" type="submit">
              Send
            </button>
          </form>
        ) : (
          <button className="perm__option perm__option--no" onClick={() => setDenying(true)}>
            No, and tell Claude what to do differently (esc)
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * The label the CLI gives its injected freeform choice; not in the payload,
 * so it cannot collide with a real option of the same name — a real "Other"
 * from the model would simply merge with it, which is what the CLI does too.
 */
const OTHER = "Other";

/**
 * The model's own multi-option prompt (`AskUserQuestion`), one section per
 * question, mirroring the terminal: options with descriptions, multi-select
 * where the model asked for it, and an injected freeform "Other".
 *
 * Answers resolve as `{ kind: "answer" }` keyed by each question's text, with
 * multi-select labels joined `", "` — the exact shape the agent reads back
 * out of `updatedInput.answers`.
 */
function QuestionCard({
  request,
  queued,
  onResolve,
}: {
  request: AgentPermission;
  queued: number;
  onResolve: (id: string, reply: ApprovalReply) => void;
}) {
  // Selections per question index; `Other` selects the freeform text below.
  const [picked, setPicked] = useState<Record<number, string[]>>({});
  const [other, setOther] = useState<Record<number, string>>({});

  // Escape declines the question the way it declines a permission — the
  // model is told and moves on, rather than waiting a day on a card the user
  // has walked away from.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onResolve(request.id, { kind: "deny", message: null });
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [request.id, onResolve]);

  const toggle = (index: number, label: string, multi: boolean) => {
    setPicked((prev) => {
      const current = prev[index] ?? [];
      const next = multi
        ? current.includes(label)
          ? current.filter((l) => l !== label)
          : [...current, label]
        : [label];
      return { ...prev, [index]: next };
    });
  };

  /** The answer string for one question, or null while it is incomplete. */
  const answerFor = (index: number): string | null => {
    const labels = picked[index] ?? [];
    if (labels.length === 0) return null;
    const resolved = labels.map((label) =>
      label === OTHER ? (other[index] ?? "").trim() : label,
    );
    if (resolved.some((text) => text === "")) return null;
    return resolved.join(", ");
  };

  const complete = request.questions.every((_, i) => answerFor(i) !== null);

  const submit = () => {
    const answers: Record<string, string> = {};
    request.questions.forEach((q, i) => {
      answers[q.question] = answerFor(i) ?? "";
    });
    onResolve(request.id, { kind: "answer", answers });
  };

  // One single-choice question answers on tap — the common case, and the
  // terminal's own feel. Anything richer gets an explicit Answer button.
  const tapSubmits = request.questions.length === 1 && !request.questions[0].multiSelect;

  return (
    <div className="perm">
      <div className="perm__head">
        <span className="perm__agent">{AGENT_NAME[request.agent] ?? request.agent}</span>
        <span className="perm__wants">has a question</span>
        {queued > 0 && <span className="perm__queue">+{queued} more</span>}
      </div>

      {request.questions.map((question, index) => (
        <div className="q" key={index}>
          <span className="q__header">{question.header}</span>
          <p className="q__text">{question.question}</p>
          <div className="q__options">
            {[...question.options, { label: OTHER, description: "" }].map((option) => {
              const selected = (picked[index] ?? []).includes(option.label);
              return (
                <button
                  key={option.label}
                  className="q__option"
                  data-selected={selected || undefined}
                  onClick={() => {
                    if (tapSubmits && option.label !== OTHER) {
                      onResolve(request.id, {
                        kind: "answer",
                        answers: { [question.question]: option.label },
                      });
                      return;
                    }
                    toggle(index, option.label, question.multiSelect);
                  }}
                >
                  <span className="q__label">{option.label}</span>
                  {option.description && (
                    <span className="q__desc">{option.description}</span>
                  )}
                </button>
              );
            })}
          </div>
          {(picked[index] ?? []).includes(OTHER) && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                if (complete) submit();
              }}
            >
              <input
                className="perm__deny-input"
                autoFocus
                value={other[index] ?? ""}
                onChange={(e) => setOther((prev) => ({ ...prev, [index]: e.target.value }))}
                placeholder="Your own answer…"
              />
            </form>
          )}
        </div>
      ))}

      {/* Tap-to-answer needs no button — except when "Other" is picked and
          there is typed text to send. */}
      {(!tapSubmits || (picked[0] ?? []).includes(OTHER)) && (
        <div className="perm__options">
          <button className="perm__option perm__option--yes" disabled={!complete} onClick={submit}>
            Answer
          </button>
        </div>
      )}
    </div>
  );
}

/** The row's dot state: busy mid-turn, live between tools, else done. */
function rowState(session: AgentSession): "busy" | "live" | "done" {
  return session.busy ? "busy" : session.tool ? "live" : "done";
}

/**
 * The row's name: what the session is working on, not what it is —
 * `aloud-tauri @ feat/agent-notch`. The agent's own name is the fallback for
 * a session with no known folder, and its model tag says the rest.
 */
function rowName(session: AgentSession): string {
  const project = session.cwd?.split("/").filter(Boolean).pop();
  if (!project) return AGENT_NAME[session.agent] ?? session.agent;
  return session.branch ? `${project} @ ${session.branch}` : project;
}

/** The `model · time` tag next to a row. */
function rowTag(session: AgentSession): string {
  return [session.model ?? session.agent, session.lastActivity && timeAgo(session.lastActivity)]
    .filter(Boolean)
    .join(" · ");
}

/**
 * Show/hide the enlarged cat preview, anchored to the given avatar element.
 * `null` stage hides it.
 */
type PeekFn = (stage: AgentStage | null, seed: string, anchor: HTMLElement | null) => void;

/**
 * The stage cat as a round avatar with a corner status badge, and the hover that
 * pops the meme up large. One component so the list row and the drilled-in
 * detail show the exact same thing — they used to carry byte-identical copies.
 */
function StageAvatar({
  session,
  stage,
  state,
  onPeek,
}: {
  session: AgentSession;
  stage: AgentStage;
  state: "busy" | "live" | "done";
  onPeek: PeekFn;
}) {
  // One seed for both the avatar and its peek, so they never disagree — it is
  // fixed while the agent stays in this stage and changes on re-entry.
  const seed = stageSeed(session.id, stage);
  const src = stageCat(stage, seed);
  return (
    <span
      className="agent-row__avatar"
      onMouseEnter={(e) => onPeek(stage, seed, e.currentTarget)}
      onMouseLeave={() => onPeek(null, "", null)}
    >
      <img className="agent-row__cat" src={src} alt="" aria-hidden width={48} height={48} decoding="async" />
      <span className="agent-row__badge" data-state={state} aria-hidden />
    </span>
  );
}

/**
 * One session in the list: the agent in bold, a `model · time` tag coloured by
 * state, the prompt or running tool, and a subagent count. The whole row is a
 * button into the detail view, and it lights up when that agent is asking.
 *
 * Ported from `agent-notch`'s `row(for:)`, made clickable.
 */
function AgentRow({
  session,
  asking,
  onSelect,
  onPeek,
}: {
  session: AgentSession;
  asking: boolean;
  onSelect: (id: string) => void;
  onPeek: PeekFn;
}) {
  const state = rowState(session);
  const stage = agentStage(session, asking);
  return (
    <li>
      <button
        className="agent-row agent-row--button"
        data-asking={asking || undefined}
        onClick={() => onSelect(session.id)}
      >
        <div className="agent-row__lead">
          <StageAvatar session={session} stage={stage} state={state} onPeek={onPeek} />
          <div className="agent-row__body">
            <div className="agent-row__head">
              <span className="agent-row__name">{rowName(session)}</span>
              <span className="agent-row__tag" data-state={state}>
                {rowTag(session)}
              </span>
            </div>
            <p className="agent-row__prompt">
          {/* When asking, that is the most important thing about the row.
              Otherwise the running tool, else the user's own latest words —
              never the ai-title under a "You:" label; the title is the
              model's and is written once at session start. */}
              {asking
                ? "⚠ wants permission"
                : (session.tool ??
                  (session.prompt ? `You: ${session.prompt}` : (session.title ?? "")))}
            </p>
            {session.subagents.length > 0 && (
              <p className="agent-row__subcount">
                ▸ {session.subagents.length} subagent{session.subagents.length === 1 ? "" : "s"}
              </p>
            )}
          </div>
        </div>
      </button>
    </li>
  );
}

/**
 * The agent sessions, once the notch has been clicked open. Each row drills
 * into a live detail view; a row whose agent is asking permission is flagged.
 */
function AgentList({
  sessions,
  permissions,
  onSelect,
  onResolve,
  onClose,
  onPeek,
}: {
  sessions: AgentSession[];
  permissions: AgentPermission[];
  onSelect: (id: string) => void;
  onResolve: (id: string, reply: ApprovalReply) => void;
  onClose: () => void;
  onPeek: PeekFn;
}) {
  const askingIds = new Set(permissions.map((p) => p.sessionId).filter(Boolean));
  return (
    <Panel title="Agents" onClose={onClose}>
      {sessions.length === 0 ? (
        <p className="agents-empty">No agents running</p>
      ) : (
        <ul className="agents">
          {sessions.map((session) => (
            <AgentRow
              session={session}
              asking={askingIds.has(session.id)}
              onSelect={onSelect}
              onPeek={onPeek}
              key={session.id}
            />
          ))}
        </ul>
      )}
      {/* A permission for an agent not in the list (rare — a hooked session the
          poll has not folded yet) still needs answering, so it falls through
          here rather than being lost. */}
      {permissions
        .filter((p) => !p.sessionId || !sessions.some((s) => s.id === p.sessionId))
        .map((p) => (
          <PermissionCard key={p.id} request={p} queued={0} onResolve={onResolve} />
        ))}
    </Panel>
  );
}

/**
 * One agent, in full: its live tool, model, elapsed and cwd, every subagent
 * expanded, and — if it is asking — the Approve / Deny card right here, so the
 * decision sits with the terminal it belongs to.
 *
 * Reads live: it is handed the session straight from the `agentSessions` state,
 * which the event listener replaces on every poll, so the tool line updates
 * without anything here subscribing.
 */
function AgentDetail({
  session,
  permission,
  onResolve,
  onBack,
  onPeek,
}: {
  session: AgentSession;
  permission: AgentPermission | null;
  onResolve: (id: string, reply: ApprovalReply) => void;
  onBack: () => void;
  onPeek: PeekFn;
}) {
  return (
    <Panel title={rowName(session)} onClose={onBack}>
      <AgentDetailBody
        session={session}
        permission={permission}
        onResolve={onResolve}
        onBack={onBack}
        onPeek={onPeek}
      />
    </Panel>
  );
}

/**
 * The detail's inner content, without the `Panel` chrome — one agent in full:
 * avatar + tag, cwd, prompt, the approve/deny card if it is asking, the live
 * "Now" tool, and every subagent. Shared verbatim between the clicked-in
 * detail (`AgentDetail`, in the opened panel) and the hover peek (`NotchAgents`
 * on dwell), so the two are the same view — one just needs no click.
 *
 * `onBack` renders the "‹ All agents" link only when given; the hover peek
 * omits it (there is nowhere to go back to). `onPeek` may be a no-op: the
 * enlarged cat-peek does not fire inside a hover peek.
 */
function AgentDetailBody({
  session,
  permission,
  onResolve,
  onBack,
  onPeek,
}: {
  session: AgentSession;
  permission: AgentPermission | null;
  onResolve: (id: string, reply: ApprovalReply) => void;
  onBack?: () => void;
  onPeek: PeekFn;
}) {
  const state = rowState(session);
  const stage = agentStage(session, permission !== null);
  return (
    <div className="agent-detail">
      {onBack && (
        <button className="agent-detail__back" onClick={onBack}>
          ‹ All agents
        </button>
      )}
      {/* The same stage avatar as the list, so the detail view matches. */}
      <div className="agent-detail__meta">
        <StageAvatar session={session} stage={stage} state={state} onPeek={onPeek} />
        <span className="agent-detail__tag" data-state={state}>
          {rowTag(session)}
        </span>
      </div>
      {session.cwd && <p className="agent-detail__cwd">{session.cwd}</p>}
      {session.prompt ? (
        <p className="agent-detail__prompt" title={session.prompt}>
          You: {truncate(session.prompt, PROMPT_CHARS)}
        </p>
      ) : (
        session.title && (
          <p className="agent-detail__prompt" title={session.title}>
            {truncate(session.title, PROMPT_CHARS)}
          </p>
        )
      )}

      {/* The card for this exact agent — the whole point of the detail view. */}
      {permission && (
        <PermissionCard request={permission} queued={0} onResolve={onResolve} />
      )}

      {/* What it is doing right now, live. */}
      <div className="agent-detail__now">
        <span className="agent-detail__label">Now</span>
        <span className="agent-detail__tool" data-idle={!session.tool || undefined}>
          {session.tool ?? (session.busy ? "thinking…" : "idle")}
        </span>
      </div>

      {session.subagents.length > 0 && (
        <div className="agent-detail__subs">
          <span className="agent-detail__label">
            Subagents ({session.subagents.length})
          </span>
          <ul className="subagents">
            {session.subagents.map((sub, i) => (
              <li className="subagent-row" key={i}>
                <span
                  className="agent-row__state"
                  data-state={sub.busy ? "busy" : "done"}
                  aria-hidden
                />
                <span className="subagent-row__label">{sub.label}</span>
                {sub.tool && <span className="subagent-row__tool">{sub.tool}</span>}
                {sub.model && <span className="subagent-row__model">{sub.model}</span>}
              </li>
            ))}
          </ul>
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
  /** Whether an ask/agent run exists — its tabs and `+` only show then. */
  runOpen,
  /** An agent is waiting on an answer: the Agents chip carries the flag. */
  asking,
  /** What tab 0 is called: the question asked, or what the agent heard. */
  title,
  onSelect,
  onClose,
  onClosed,
  onAdd,
}: {
  tabs: Tab[];
  active: number;
  runOpen: boolean;
  asking: boolean;
  title: string;
  onSelect: (id: number) => void;
  onClose: (id: number) => void;
  onClosed: (id: number) => void;
  onAdd: () => void;
}) {
  return (
    <div className="tabs">
      {/* The agents' fixed slot: leftmost, always present, never closable —
          home base for the strip, the way a browser pins its first tab. New
          tabs open to its right. */}
      <div className="tab" data-motion="in">
        <div
          className="tab__chip"
          data-active={active === AGENTS_TAB || undefined}
          data-asking={asking || undefined}
        >
          <button className="tab__select" onClick={() => onSelect(AGENTS_TAB)}>
            Agents
          </button>
        </div>
      </div>
      {/* The run's own tab only exists while a run does; blank tabs belong
          to the strip and open from anywhere — the notch is one surface. */}
      {tabs
        .filter((tab) => runOpen || tab.blank)
        .map((tab, i) => (
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
              {/* A blank tab is a coding-agent composer, so name it one —
                  numbered by position, not id, so the strip reads 1..n after a
                  close and two of them stay switchable-between. */}
              {tab.blank ? `Agent ${i + 1}` : title}
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

/** Where the last-used repo path is remembered between sessions, so the
 *  common case (same repo again) is one field pre-filled rather than retyped.
 *  ponytail: one string in localStorage, not a recent-repos list — add the
 *  list if people work across many repos often enough to miss it. */
const LAST_REPO_KEY = "aloud.codingAgent.lastRepo";

/**
 * A searchable dropdown for the composer — a trigger plus a cmdk list, shared by
 * the Repo and Branch fields. The trigger wears the composer's field look
 * (`composer__trigger`); the list dark-renders for free through the overlay's
 * floating tokens. `allowCustom` lets Repo accept a typed path that isn't in
 * the list (a repo outside the scanned roots); Branch leaves it off.
 *
 * The open list renders **inline**, not in a Radix Popover portal. The overlay
 * is a click-through window whose clickable region is exactly the panel box the
 * webview reports; a portal to `document.body` lands *outside* that box, so
 * every click on it fell through and dismissed the notch. Rendered inline, the
 * list is part of the panel's measured content — `useNotchPanel` grows the box
 * to include it, so its clicks land inside and it stops closing the notch.
 */
const Combobox = React.forwardRef<
  HTMLButtonElement,
  {
    value: string;
    onChange: (value: string) => void;
    options: string[];
    placeholder: string;
    empty: string;
    disabled?: boolean;
    allowCustom?: boolean;
    /** Show a shimmer in the trigger instead of a value, while options load. */
    loading?: boolean;
  }
>(function Combobox(
  { value, onChange, options, placeholder, empty, disabled, allowCustom, loading },
  triggerRef,
) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");

  const pick = (v: string) => {
    onChange(v);
    setOpen(false);
    setQuery("");
  };

  return (
    <div className="composer__combobox">
      <button
        ref={triggerRef}
        className="composer__trigger"
        type="button"
        disabled={disabled}
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        {loading ? (
          <Shimmer as="span">Loading…</Shimmer>
        ) : (
          <span className={value ? undefined : "composer__trigger-placeholder"}>
            {value || placeholder}
          </span>
        )}
        <ChevronsUpDownIcon className="composer__trigger-icon" />
      </button>
      {open ? (
        <Command
          className="composer__list"
          // Let a typed path through even when it matches nothing in the list.
          shouldFilter={!allowCustom || options.length > 0}
          // Escape closes the list without closing the whole composer.
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              setOpen(false);
            }
          }}
        >
          <CommandInput
            placeholder={placeholder}
            value={query}
            onValueChange={setQuery}
            autoFocus
          />
          <CommandList>
            <CommandEmpty>
              {allowCustom && query.trim() ? (
                <button className="composer__use-typed" onClick={() => pick(query.trim())}>
                  Use “{query.trim()}”
                </button>
              ) : (
                empty
              )}
            </CommandEmpty>
            {options.map((option) => (
              <CommandItem key={option} value={option} onSelect={() => pick(option)}>
                {option}
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      ) : null}
    </div>
  );
});

/**
 * What a blank tab holds: the composer for starting a coding agent. Pick a repo,
 * pick one of its branches, say what to do, and Rust opens a Terminal tab that
 * checks that branch out in the repo and runs the agent on the task there (no
 * worktree — it edits the repo directly). The running session then shows up
 * under the Agents tab through the same discovery every other session uses.
 *
 * The three choices are independent: the task is the agent's instruction
 * verbatim, and the branch is picked, not derived from it. Branches are fetched
 * when the repo settles, so the dropdown reflects the chosen repo.
 *
 * A `Panel` like `Answer`/`AgentRun` so it inherits the strip's `+`, close, and
 * the always-dark chrome; the panel's own spring settles around the fields, so
 * revealing the branch row simply grows the notch — no animation of its own.
 */
function CodingAgentComposer({
  bodyRef,
  onClose,
  onAddTab,
}: {
  bodyRef: React.RefObject<HTMLDivElement | null>;
  onClose: () => void;
  onAddTab?: () => void;
}) {
  const [repo, setRepo] = useState(() => localStorage.getItem(LAST_REPO_KEY) ?? "");
  const [branch, setBranch] = useState("");
  const [task, setTask] = useState("");
  const [agent, setAgent] = useState<CodingAgent>("claude");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [started, setStarted] = useState<string | null>(null);
  const [hooksInstalled, setHooksInstalled] = useState<boolean | null>(null);
  const [repos, setRepos] = useState<LocalRepo[]>([]);
  // The chosen repo's branches, fetched when the repo settles. Empty until a
  // repo resolves; `loadingBranches` distinguishes "not fetched" from "none".
  const [branches, setBranches] = useState<string[]>([]);
  const [loadingBranches, setLoadingBranches] = useState(false);
  // The repo trigger, focused on open so the flow starts on the keyboard.
  const repoRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    void commands.agentHookStatus().then((res) => {
      setHooksInstalled(res.status === "ok" ? res.data.installed.length > 0 : true);
    });
    void commands.listLocalRepos().then((res) => {
      if (res.status === "ok") setRepos(res.data);
    });
    // DOM focus only — the overlay is already key (show_panel took focus).
    repoRef.current?.focus();
  }, []);

  // A picked name maps back to its path; anything else is a literal path (the
  // free-text escape hatch for a repo outside the scanned roots).
  const resolveRepo = useCallback(
    (value: string) => repos.find((r) => r.name === value)?.path ?? value,
    [repos],
  );

  // Fetch the repo's branches once it resolves to a real path. Keyed on the
  // resolved path so switching repos refetches; a stale response for a repo the
  // user has since changed away from is dropped by the `active` guard.
  const repoPath = repo.trim() ? resolveRepo(repo.trim()) : "";
  useEffect(() => {
    if (!repoPath) {
      setBranches([]);
      return;
    }
    let active = true;
    setLoadingBranches(true);
    void commands.listRepoBranches(repoPath).then((res) => {
      if (!active) return;
      setLoadingBranches(false);
      const list = res.status === "ok" ? res.data : [];
      setBranches(list);
      // Default to the first branch (most-recently-committed) so the common
      // case needs no branch click — but never clobber a choice already made.
      setBranch((b) => (b && list.includes(b) ? b : list[0] ?? ""));
    });
    return () => {
      active = false;
    };
  }, [repoPath]);

  const canStart = !busy && repoPath !== "" && branch !== "" && task.trim() !== "";

  const go = useCallback(() => {
    if (!canStart) return;
    setBusy(true);
    setError(null);
    localStorage.setItem(LAST_REPO_KEY, repo.trim());
    void commands.spawnCodingAgent(repoPath, branch, task.trim(), agent).then((res) => {
      setBusy(false);
      if (res.status === "ok") setStarted(res.data);
      else setError(res.error);
    });
  }, [canStart, repoPath, repo, branch, task, agent]);

  // "Start another" from the success state: keep the repo (you usually queue
  // several in the same one), clear the task, land focus back on the task.
  const startAnother = () => {
    setStarted(null);
    setTask("");
    setError(null);
  };

  return (
    <Panel title="New coding agent" bodyRef={bodyRef} onClose={onClose} onAddTab={onAddTab}>
      {started ? (
        <p className="ask__done">
          Started on <span className="ask__branch">{started}</span>. It will
          appear under Agents in a moment.{" "}
          <button className="composer__link" onClick={startAnother}>
            Start another
          </button>
        </p>
      ) : (
        <div
          className="composer"
          // Cmd/Ctrl+Enter starts from anywhere in the composer, not just the
          // Task field — the whole form is one submit.
          onKeyDown={(e) => {
            if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
              e.preventDefault();
              go();
            }
          }}
        >
          <div className="composer__field">
            <span className="composer__label">Repo</span>
            <Combobox
              ref={repoRef}
              value={repo}
              onChange={setRepo}
              options={repos.map((r) => r.name)}
              placeholder={repos.length ? "Pick a repo" : "~/dev/my-app"}
              empty="No repos found"
              disabled={busy}
              // A repo outside the scanned roots can still be typed as a path.
              allowCustom
            />
          </div>

          {/* The branch row appears once a repo is chosen; the panel grows to
              fit it via useNotchPanel's spring — the reveal IS the motion. */}
          {repoPath ? (
            <div className="composer__field composer__field--branch">
              <span className="composer__label">Branch</span>
              <Combobox
                value={branch}
                onChange={setBranch}
                options={branches}
                placeholder="Pick a branch"
                empty="No branches found"
                loading={loadingBranches}
                disabled={busy || loadingBranches || branches.length === 0}
              />
            </div>
          ) : null}

          <label className="composer__field">
            <span className="composer__label">Task</span>
            <input
              className="composer__input"
              placeholder="What should it do?"
              value={task}
              disabled={busy}
              onChange={(e) => setTask(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && go()}
            />
          </label>

          <div className="composer__row">
            <div
              className="composer__agents"
              role="radiogroup"
              aria-label="Agent"
              // Arrow keys move between claude/codex, the radiogroup convention.
              onKeyDown={(e) => {
                if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
                  setAgent((a) => (a === "claude" ? "codex" : "claude"));
                }
              }}
            >
              {(["claude", "codex"] as const).map((option) => (
                <button
                  key={option}
                  className="composer__agent"
                  role="radio"
                  aria-checked={agent === option}
                  data-active={agent === option || undefined}
                  // Only the selected radio is tab-focusable; arrows move within.
                  tabIndex={agent === option ? 0 : -1}
                  disabled={busy}
                  onClick={() => setAgent(option)}
                >
                  {option}
                </button>
              ))}
            </div>
            <button className="composer__go" disabled={!canStart} onClick={go}>
              {busy ? "Starting…" : "Start"}
            </button>
          </div>

          {error ? <p className="ask__error">{error}</p> : null}
          {hooksInstalled === false ? (
            <p className="composer__hint">
              Approvals won't reach the notch until agent hooks are installed.{" "}
              <button
                className="composer__link"
                onClick={() => void commands.installAgentHooks().then(() => setHooksInstalled(true))}
              >
                Install hooks
              </button>
            </p>
          ) : null}
        </div>
      )}
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
