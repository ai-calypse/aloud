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
  commands,
  events,
  type AgentPermission,
  type AgentSession,
  type ApprovalReply,
  type OverlayStyle,
  type Phase,
} from "./lib/bindings";
import {
  withCall,
  withDecision,
  withResult,
  withoutPlanned,
  type Run,
} from "./lib/agent-steps";
import { agentStage, syncStages, type AgentStage } from "./lib/agent-stage";
import { truncate } from "./lib/format";
import { PREVIEW_EVENT, type PreviewCommand } from "./lib/overlay-preview";
import { CatSettingsProvider } from "./lib/cat-settings";
import { listen } from "@tauri-apps/api/event";
import { AgentList, AgentDetail } from "./features/overlay/AgentList";
import { AgentRun } from "./features/overlay/AgentRun";
import { CodingAgentComposer } from "./features/overlay/CodingAgentComposer";
import {
  ListeningCat,
  CatPeek,
  AmbientSign,
  NotchAgents,
} from "./features/overlay/NotchAgents";
import {
  type Tab,
  FIRST_TABS,
  AGENTS_TAB,
  MAX_TABS,
  TabStrip,
} from "./features/overlay/TabStrip";
import {
  PARTIAL_PREVIEW,
  PARTIAL_MS,
  AGENT_PREVIEW,
  FALLBACK_PREVIEW,
} from "./features/overlay/previews";
import { useNotchPanel } from "./features/overlay/useNotchPanel";
// Streamdown styles the agent's answer with Tailwind utilities — see the
// `@source` note in index.css.
import "./index.css";
import "./styles/overlay.css";

const LABEL: Record<Phase, string> = {
  idle: "",
  // Always-listening at rest: the wings' dot says it; there is no word to show.
  listening: "",
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
  const [agent, setAgent] = useState<Run | null>(null);
  // The notch opened by clicking it rather than by speaking. Its own state
  // because it belongs to no dictation and must survive `run` changing.
  const [browsing, setBrowsing] = useState(false);
  // Tool calls awaiting Approve / Deny, newest first — a swarm can ask several
  // at once, so this is a queue, not a single card. Keyed by tool_use_id.
  const [permissions, setPermissions] = useState<AgentPermission[]>([]);
  // The agent row the user drilled into, by session id, or null for the list.
  const [selected, setSelected] = useState<string | null>(null);
  // The cat toggles from Settings. The overlay is its own window and does not
  // otherwise read settings, so it fetches them on mount and re-reads on every
  // style event (fired on each overlay show) and each new dictation — a toggle
  // change lands by the next time a cat would appear, which is soon enough.
  // Default on so the very first paint (before the fetch resolves) shows cats,
  // matching the Settings default rather than flashing them off then on.
  // Declared here (above `showPeek`) because that callback reads them.
  const [catImages, setCatImages] = useState(true);
  const [catImagesDictation, setCatImagesDictation] = useState(true);
  const [catImagesAgents, setCatImagesAgents] = useState(true);
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
      // No cat to enlarge when agent cats are off — the fallback disc has
      // nothing to reveal, so the peek is suppressed entirely.
      if (!catImages || !catImagesAgents) return;
      setPeek(stage && anchor ? { stage, seed, rect: anchor.getBoundingClientRect() } : null);
    },
    [catImages, catImagesAgents],
  );
  // Any of them owns the panel; almost everything below only cares that one
  // does. A pending permission counts: the card has to be on screen to answer.
  // A pending permission is deliberately NOT part of this: a card *arriving*
  // opens the panel (the event listener sets `browsing`), but it must not
  // *hold* it — deriving openness from the queue pinned the panel so hard
  // that the cross, click-away and Escape all read as broken. Closing with a
  // card pending is fine: the hook waits (24h), the card stays queued under
  // the Agents tab's dot, and reopening the notch finds it again.
  const panelOpen = agent !== null || browsing;
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
    if (panelOpen && !agent) setActiveTab(AGENTS_TAB);
  }, [panelOpen, agent]);

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
          danger: true,
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
          danger: false,
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
          danger: false,
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

  // Read the cat toggles from Settings. Called on mount and on each style event
  // and new dictation; `invoke` throws synchronously in the browser preview
  // (no Tauri), where the defaults stand.
  const refreshCatSettings = useCallback(() => {
    try {
      void commands.getSettings().then((s) => {
        setCatImages(s.catImages ?? true);
        setCatImagesDictation(s.catImagesDictation ?? true);
        setCatImagesAgents(s.catImagesAgents ?? true);
      });
    } catch {
      /* browser preview: no Tauri, keep the defaults */
    }
  }, []);
  useEffect(refreshCatSettings, [refreshCatSettings]);

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
      const home = agent ? 0 : AGENTS_TAB;
      setActiveTab((active) => (active === id ? home : active));
    },
    [agent],
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
  // Only while words are being said — a dictation, or always-listening — never
  // under "Transcribing…", which is about the whole utterance being re-read.
  const transcript =
    !panelOpen && (phase === "recording" || phase === "listening") && partial.trim().length > 0;

  // The "I am listening" cat shows ONLY while the app is actively listening to
  // you — the live dictation, `recording`/`transcribing`. The meme's caption is
  // true then and only then; once the answer is on screen the app has stopped
  // listening, so the cat comes down. Never at rest, never over a result, never
  // in meetings (a separate window this component does not draw). Gated on the
  // Settings > Cats toggles — the master and the per-place switch both apply.
  const dictating =
    catImages &&
    catImagesDictation &&
    !panelOpen &&
    (phase === "recording" || phase === "transcribing");

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
        // …unless an answer is on screen: the run ends on `idle`, which is
        // exactly when the answer wants reading.
        if (!openRef.current) setDismissing(true);
        return;
      }

      setPhase(e.payload);
      // A phase arriving means the window is staying up, cancelling an exit.
      setDismissing(false);
      // Back to listening after a request: the last one's words and the drop
      // they caused are over, so the wings come back up into the menu bar.
      if (e.payload === "listening") {
        setError(null);
        setPartial("");
        setDroppedDown(false);
      }
      if (e.payload === "recording") {
        setError(null);
        // A dictation started over the top of an answer takes the panel back —
        // including from the agents surface, or the pill and the panel fight
        // over the same strip.
        setAgent(null);
        setBrowsing(false);
        setSelected(null);
        setPartial("");
        setDroppedDown(false);
        setRun((previous) => previous + 1);
      }
    });
    // A dismiss is Rust declaring the panel over, so the panel *state* goes
    // with it. Leaving `agent` latched here is what made the panel
    // spring back open the moment `dismissing` reset — and the second click
    // on the cross was then a no-op, because Rust's `end_panel` had already
    // cleared its side and had nothing left to close.
    const unlistenDismiss = events.overlayDismiss.listen(() => {
      setDismissing(true);
      setAgent(null);
      setBrowsing(false);
      setSelected(null);
    });
    const unlistenError = events.overlayError.listen((e) => setError(e.payload));
    const unlistenStyle = events.overlayStyle.listen((e) => {
      setStyle(e.payload.style);
      // Fired on every overlay show, so it doubles as the "settings may have
      // changed" signal for the cat toggles — cheap, and the freshest point.
      refreshCatSettings();
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
    // The Settings window fires this on save; the overlay is a separate window
    // with its own settings copy, so this is how a Cats toggle lands live
    // instead of only on the next overlay show.
    const unlistenSettings = listen("settings-changed", refreshCatSettings);
    return () => {
      unlistenPhase.then((off) => off());
      unlistenDismiss.then((off) => off());
      unlistenError.then((off) => off());
      unlistenStyle.then((off) => off());
      unlistenDownload.then((off) => off());
      unlistenPartial.then((off) => off());
      unlistenSettings.then((off) => off());
    };
  }, []);

  // The tuning page's hatch; see PreviewCommand. Inert in the real app.
  useEffect(() => {
    let typing = 0;
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
            setAgent(null);
          setError(null);
          setPartial("");
          setDroppedDown(false);
          setRun((previous) => previous + 1);
        }
      }
      if (command.partial === "stream") dictate();
      if (command.agent === "run") {
        setAgent({
          ...AGENT_PREVIEW,
          steps: [AGENT_PREVIEW.steps[0]],
          finished:
            "Sent the release notes to Priya. The reply to Sam is drafted and waiting in Gmail.",
        });
      }
      if (command.agent === "pending") {
        setAgent(AGENT_PREVIEW);
      }
      if (command.agent === "planned") {
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
              result: { success: true, message: "no results", show: null },
            },
            {
              name: "GMAIL_FETCH_EMAILS",
              app: "Gmail",
              args: { query: "Genghis Khan" },
              requiresConfirmation: false,
              result: { success: true, message: "no results", show: null },
            },
            {
              name: "GMAIL_FETCH_EMAILS",
              app: "Gmail",
              args: { query: "from:Genghis" },
              requiresConfirmation: false,
              result: { success: true, message: "no results", show: null },
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
        setAgent({
          heard: "Send hi to Charlie Yang",
          steps: [
            {
              name: "GOOGLECONTACTS_SEARCH_CONTACTS",
              app: "Google Contacts",
              args: { query: "Charlie Yang" },
              requiresConfirmation: false,
              result: { success: true, message: "no results", show: null },
            },
            {
              name: "GMAIL_FETCH_EMAILS",
              app: "Gmail",
              args: { query: "Charlie Yang" },
              requiresConfirmation: false,
              result: { success: true, message: "no results", show: null },
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
        setAgent({
          heard: "Send hi to Sam",
          steps: [
            {
              name: "GOOGLECONTACTS_SEARCH_CONTACTS",
              app: "Google Contacts",
              args: { query: "Sam" },
              requiresConfirmation: false,
              result: { success: true, message: "3 results", show: null },
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
      if (command.agent === "fallback") setAgent(FALLBACK_PREVIEW);
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
      if (!agent) {
        void commands.closeBrowse();
        setBrowsing(false);
        setSelected(null);
        return;
      }
      // `closeAgent` also abandons a pending confirmation, so Escape reads as
      // "no" rather than leaving a write waiting on a channel nobody answers.
      void commands.closeAgent();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [panelOpen, agent]);

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
    // Every agent-cat site (strip, cards, list, detail, peek) reads its
    // visibility from here rather than taking a prop each. `agents` folds in
    // the master switch, so a consumer asks one question.
    <CatSettingsProvider
      value={{
        all: catImages,
        dictation: catImages && catImagesDictation,
        agents: catImages && catImagesAgents,
      }}
    >
    {/* The exit is a state of the whole surface, not of one style. */}
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
          }${dictating ? " drop-pill__body--cat" : ""}`}
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
            // are a tab on it, and a dictation's agent panel is another —
            // so escaping one never takes the other away.
            <>
              <TabStrip
                tabs={tabs}
                active={activeTab}
                runOpen={agent !== null}
                asking={permissions.length > 0}
                title={
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
                      if (agent) setActiveTab(0);
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
              ) : agent ? (
                <AgentRun
                  key={agent.heard}
                  run={agent}
                  bodyRef={answerRef}
                  onAddTab={tabs.length < MAX_TABS ? addTab : undefined}
                  onDecide={(confirmed) =>
                    setAgent((r) => r && { ...r, steps: withDecision(r.steps, confirmed) })
                  }
                />
              ) : null}
            </>
          ) : (
            <>
              {/* The meme is listening while the app is. See ListeningCat. */}
              {dictating && <ListeningCat />}
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
            </>
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
    </CatSettingsProvider>
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

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
