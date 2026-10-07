import React, {
  useCallback,
  useEffect,
  useLayoutEffect,
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
  type Island,
  type IslandAction,
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
import { NotchToast, toastForm } from "./features/overlay/NotchToast";
import { useDwell } from "./features/overlay/useDwell";
import { ListeningCat, CatPeek } from "./features/overlay/NotchAgents";
import { AgentRun } from "./features/overlay/AgentRun";
import { NotchShell } from "./features/notch/NotchShell";
import { HOVER_INTENT_MS, homeNav, shellState } from "./features/notch/hover";
import { useHoverIntent } from "./features/notch/useHoverIntent";
import { useVoiceRuns } from "./features/notch/voice-runs";
import { IslandPill } from "./features/notch/IslandPill";
import { runAction } from "./features/notch/actions";
import { NotchProvider } from "./features/notch/context";
import { applyIsland, dwellMs, opensOnHover, takeoverTier } from "./features/notch/islands";
import { loadNav, saveNav, type Nav, type SectionId } from "./features/notch/sections";
import {
  PARTIAL_PREVIEW,
  PARTIAL_MS,
  AGENT_PREVIEW,
  VOICE_RUN_PREVIEWS,
  CARD_PREVIEWS,
  CONTACT_PREVIEW,
  FINDER_PREVIEWS,
  livePreviews,
  TAKEOVER_PREVIEWS,
  TOAST_PREVIEWS,
  withIcon,
} from "./features/overlay/previews";
import { useNotchPanel } from "./features/overlay/useNotchPanel";
// The notch panel's faces, bundled: the CSP allows no font host.
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
// Streamdown styles the agent's answer with Tailwind utilities — see the
// `@source` note in index.css.
import "./index.css";
import "./styles/overlay.css";
import "./styles/notch.css";
import "./styles/events.css";
import "./styles/nudges.css";

const LABEL: Record<Phase, string> = {
  idle: "",
  // Always-listening at rest: the wings' dot says it; there is no word to show.
  listening: "",
  recording: "Listening…",
  transcribing: "Transcribing…",
  done: "Pasted",
  saved: "Saved to journal",
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
  // The island queue's copy, ranked by Rust (see `applyIsland`).
  const [islands, setIslands] = useState<Island[]>([]);
  // The alert island on screen — the confirmation chip a run of writes ends
  // on, and any later alert. Not a panel: it holds no focus and takes no
  // keys, and it stays mounted through its own exit — it is cleared only once
  // the collapse has played (the dismissing timer). Keyed, so an alert
  // replacing another restarts its clock.
  const [toast, setToast] = useState<{ value: Island; key: number } | null>(null);
  const toastKey = useRef(0);
  const toastRef = useRef(toast);
  toastRef.current = toast;
  // A whisper alongside: a glyph and one thing beside the camera at rest, the
  // top row of a peek, an echo under a card. Not a panel and not a toast: it
  // claims no window (Rust leaves the resting one alone) and its own clock
  // takes it away. Keyed so a fresh whisper restarts its clock; the same id
  // updating in place keeps its key, which is how a volume bar moves instead
  // of remounting.
  const [whisper, setWhisper] = useState<{ value: Island; key: number } | null>(null);
  const whisperKey = useRef(0);
  const whisperRef = useRef(whisper);
  whisperRef.current = whisper;
  // How many takeovers wait for their turn (Rust's line): "+N".
  const [waiting, setWaiting] = useState(0);
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
  // Settings › Labs. Off, this window is only the dictation pill and a voice
  // run's panel: no shell, no resting islands, no hover. Off until read, so a
  // notch-off install never flashes one; Rust shows the resting notch only
  // after it is on, and the read lands well before a pointer can dwell. The
  // ref is for the listeners below, which subscribe once.
  const [notchOn, setNotchOn] = useState(false);
  const notchRef = useRef(notchOn);
  notchRef.current = notchOn;
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
  // A run alone is the compact panel (just the run, grown from the notch); the
  // full sections shell is only for someone who asked for it — a click, a hover
  // that was then clicked into, a voice command naming a section, an approval.
  const compact = agent !== null && !browsing;
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

  // Where the panel is, and where the user last chose to be. A run or an
  // approval takes the panel to itself without moving the remembered place,
  // so the next click on the notch opens where the user was, not wherever a
  // command happened to land.
  const [nav, setNav] = useState<Nav>(loadNav);
  const navRef = useRef(nav);
  navRef.current = nav;
  const chosen = useRef(nav);
  const navigate = useCallback((section: SectionId, sub?: string) => {
    const current = navRef.current;
    const next = { section, subs: sub ? { ...current.subs, [section]: sub } : current.subs };
    chosen.current = next;
    saveNav(next);
    setNav(next);
  }, []);
  const takeFocus = useCallback((section: SectionId, sub?: string) => {
    setNav((current) => ({
      section,
      subs: sub ? { ...current.subs, [section]: sub } : current.subs,
    }));
  }, []);
  // ⌘K, open in the panel's body. Here rather than in the shell so a closed
  // panel cannot reopen onto a half-typed search.
  const [palette, setPalette] = useState(false);
  // A run takes the shell, if it is open, to its Voice view (otherwise the run
  // is the compact panel and the section is only where "open in Agents" lands);
  // keyed on the utterance, so it fires once per run and the user can still
  // navigate away while it works. A layout effect so the panel never paints a
  // frame of the wrong section — pure state, no `invoke`, so it cannot stall
  // the paint it precedes.
  const runKey = agent?.heard;
  useLayoutEffect(() => {
    if (runKey !== undefined) takeFocus("agents", "voice");
  }, [runKey, takeFocus]);

  // Dev/QA: `?agents=2` fakes a session list and `?hover` holds the pointer on
  // the notch (Home opens after the hover delay), so the resting notch's states
  // are reachable in a browser. The real ones only arrive from Rust, and the
  // hover only from a cursor poll. `?runs` fills Agents › Voice's history.
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
    // `?notch=200x32` stands in for the cutout Rust measures and sends with
    // every style event, so a browser preview draws the hardware's shape.
    const [notchW, notchH] = (params.get("notch") ?? "").split("x").map(Number);
    if (notchW > 0 && notchH > 0) {
      root.current?.style.setProperty("--notch-w", `${notchW}px`);
      root.current?.style.setProperty("--notch-h", `${notchH}px`);
    }
    // The agents' island is Rust's, so the preview stands it in for `?agents`
    // (the cats beside the camera); `?islands=3` adds the fixtures' others.
    const live = Number(params.get("islands"));
    if (live > 0) setIslands(livePreviews(live));
    else if (count > 0) setIslands(livePreviews(1));
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
        setNotchOn(s.labs?.notch ?? false);
      });
    } catch {
      /* browser preview: no Tauri, keep the defaults */
    }
  }, []);
  useEffect(refreshCatSettings, [refreshCatSettings]);

  // Agents, hover and permission requests, all independent of any dictation.
  // An alert island arriving: a whisper takes its side slot and touches nothing
  // else; anything bigger is the toast, and replaces whatever panel was up (Rust
  // only sends one where no panel is being read — `overlay::show_toast`). Also
  // what the tuner's fixtures go through, so the preview is the real routing.
  const takeOver = useCallback((value: Island) => {
    if (takeoverTier(value) === "whisper" && !value.shrunk) {
      const same = whisperRef.current?.value.id === value.id;
      setWhisper((now) => ({ value, key: same && now ? now.key : ++whisperKey.current }));
      return;
    }
    setToast({ value, key: ++toastKey.current });
    setAgent(null);
    setBrowsing(false);
    setSelected(null);
    setDismissing(false);
  }, []);

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
      // The cards live only in the shell, so never with the notch off (Rust
      // does not park a request then; this covers one already in flight).
      if (e.payload.open && notchRef.current) {
        setBrowsing(true);
        setSelected(e.payload.sessionId ?? e.payload.id);
      }
    });
    // Answered here, abandoned by the agent, or the app deciding for us: the
    // card comes down the same way in every case.
    const resolvedOff = events.agentPermissionResolved.listen((e) =>
      setPermissions((queue) => queue.filter((p) => p.id !== e.payload.id)),
    );
    // The island queue. An alert arriving is what the agent's "toast" event
    // used to be: the run's own panel collapses into the chip (the notch
    // panel's spring retargets from the panel's size to the chip's) and a
    // quiet run's grows out of the cutout. Rust only pushes one where no
    // other panel is being read (`overlay::show_toast`).
    const islandOff = events.island.listen(({ payload }) => {
      setIslands((list) => applyIsland(list, payload));
      const { change } = payload;
      if (change.op !== "remove" && change.value.kind === "alert") {
        takeOver(change.value);
      }
      // Taken down from Rust's side (its backstop): leave the way the
      // clock would.
      if (change.op === "remove" && toastRef.current?.value.id === change.value) {
        setDismissing(true);
      }
      if (change.op === "remove" && whisperRef.current?.value.id === change.value) {
        setWhisper(null);
      }
    });
    const waitingOff = events.islandsWaiting.listen(({ payload }) => setWaiting(payload.count));
    // "Show my agents": the shell, at that section. Rust has already taken the
    // window and the keyboard (`open_notch`). A jump the user asked for by
    // voice, not a place they chose, so the next click still opens where they
    // last were.
    const notchOpenOff = events.notchOpen.listen(({ payload }) => {
      if (!notchRef.current) return;
      setAgent(null);
      setToast(null);
      setSelected(null);
      setDismissing(false);
      setBrowsing(true);
      takeFocus(payload.section as SectionId, payload.sub ?? undefined);
    });
    // The event carries changes only; islands pushed before this window
    // listened arrive with the snapshot.
    try {
      void commands.islands().then(setIslands);
    } catch {
      /* browser preview: no Tauri, no islands */
    }
    return () => {
      void sessionsOff.then((off) => off());
      void hoverOff.then((off) => off());
      void permOff.then((off) => off());
      void resolvedOff.then((off) => off());
      void islandOff.then((off) => off());
      void waitingOff.then((off) => off());
      void notchOpenOff.then((off) => off());
    };
  }, [takeOver, takeFocus]);

  // Answer, then drop it locally — the resolved event will also drop it, and
  // whichever lands first wins. Removing it here keeps the next card from
  // waiting on the round-trip.
  const resolvePermission = useCallback((id: string, reply: ApprovalReply) => {
    void commands.resolvePermission(id, reply);
    setPermissions((queue) => queue.filter((p) => p.id !== id));
  }, []);

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
    toast === null &&
    (phase === "recording" || phase === "transcribing");

  /**
   * The style actually drawn: a wing has nowhere to put a sentence, so the
   * moment there are words it hands over to the drop-down panel. `droppedDown`
   * latches for the rest of the run rather than tracking the words — the
   * transcript is gone by `transcribing`, and springing back up to a menu-bar
   * tab there would undo the drop just watched.
   *
   * At rest it is always the drop pill, whatever the last dictation drew: the
   * resting notch is the cutout (Rust's `show_notch` says so too), and the
   * live islands and the hover peek live in it. Before islands the agents'
   * strip drew outside it, so a notch left in `wings` after a dictation lost
   * only the hover; now it would lose the agents too.
   */
  // Nothing is being said and no panel is up: the notch is just sitting there.
  const resting = !panelOpen && phase === "idle" && !dismissing && toast === null;
  const effectiveStyle: OverlayStyle =
    style === "wings" && (droppedDown || panelOpen || toast !== null || resting)
      ? "dropPill"
      : style;
  // What the resting notch shows: the live islands, beside the camera — the
  // coding agents' cats while they work or ask (Rust's `agents` island), and
  // whatever features push later. Resting under the pointer opens Home.
  const live = islands.filter((island) => island.kind === "live");
  // A whisper takes the resting notch for its moment; over a toast it is part
  // of the toast instead (a peek's top row, an echo under a card), and while a
  // dictation or a panel has the window it waits, its clock stopped.
  const whisperNow = notchOn && resting && toast === null && whisper !== null;
  // Pointing at a whisper that has a button opens it into a peek. That is the
  // hover the user is answering, so it outranks Home's: Home opens once the
  // whisper has gone and the pointer is still there.
  const whisperOpen = whisperNow && hovering && whisper !== null && opensOnHover(whisper.value);
  // Resting under a pointer that has stayed: Home opens in place (`shellState`).
  // Only what is clicked or typed into stays when the pointer leaves.
  const dwelled = useHoverIntent(notchOn && resting && hovering, HOVER_INTENT_MS);
  // With the notch off in Labs there is no shell to open, however it is asked.
  const shell = notchOn
    ? shellState({ browsing, resting, hovering, dwelled, whisperOpen })
    : "closed";
  const peeking = shell === "peek";
  // Held a beat after the pointer leaves, so Home collapses into the cutout on
  // the panel's spring rather than snapping (the same as the live pill's exit).
  const peekShown = useLinger(peeking, 600);
  const peekLeaving = peekShown && !peeking && resting;
  const shellShown = shell !== "closed" || peekLeaving;
  // What was said before now, for Agents › Voice. Read when the shell opens and
  // when the run on screen ends (Rust writes its row just before it says so).
  const { entries: runs, setEntries: setRuns, remove: removeRun } = useVoiceRuns(
    `${shellShown}|${agent?.heard}|${agent?.finished !== undefined || agent?.error !== undefined}`,
  );
  useEffect(() => {
    if (new URLSearchParams(location.search).has("runs")) setRuns(VOICE_RUN_PREVIEWS);
  }, [setRuns]);
  // Closed, the next open is a click — which lands where the user last chose.
  // A hover-opened Home counts as open until its exit has played, or the
  // panel would swap sections while it shrinks.
  useEffect(() => {
    if (!panelOpen && !peekShown) {
      setNav(chosen.current);
      setPalette(false);
    }
  }, [panelOpen, peekShown]);
  // Home, whichever section was chosen: a hover is a glance at what is live.
  useLayoutEffect(() => {
    if (peeking) setNav(homeNav(chosen.current));
  }, [peeking]);
  // Clicking into a hover-opened Home, or typing in it, makes it a panel like
  // one opened by a click. (Typing only reaches here once the window is key.)
  useEffect(() => {
    if (!peeking) return;
    const commit = () => setBrowsing(true);
    window.addEventListener("keydown", commit);
    return () => window.removeEventListener("keydown", commit);
  }, [peeking]);
  // Takeovers waiting their turn show as "+N" even on a bare notch.
  const liveNow = notchOn && resting && (live.length > 0 || whisperNow || waiting > 0);
  // Held up a beat after the last one goes, so the pill collapses into the
  // cutout on the panel's spring (`pillLeaving` below) rather than snapping.
  const pillShown = useLinger(liveNow, 600);
  const pillLeaving = pillShown && !liveNow;
  // What the pill draws while it leaves: the last activities or whisper,
  // fading out as the shape collapses under them, rather than an empty pill
  // doing so.
  const lastLive = useRef(live);
  if (live.length > 0) lastLive.current = live;
  const lastWhisper = useRef<Island | null>(null);
  if (whisperNow && whisper) lastWhisper.current = whisper.value;
  else if (live.length > 0) lastWhisper.current = null;
  const pillWhisper = whisperNow && whisper ? whisper.value : liveNow ? null : lastWhisper.current;
  // The whisper's own clock: stopped while it cannot be seen and while the
  // pointer is on one that opens, like a toast's. Rust's backstop covers a
  // webview that never gets to show it.
  const endWhisper = useCallback(() => {
    const id = whisperRef.current?.value.id;
    setWhisper(null);
    try {
      if (id) void commands.dismissIsland(id);
    } catch {
      /* browser preview: no queue to tell */
    }
  }, []);
  useDwell(
    whisper ? dwellMs(whisper.value) : null,
    !(whisperNow || (whisper !== null && toast !== null)) || whisperOpen,
    endWhisper,
    whisper?.key,
  );
  // What a toast is drawn as: a side capsule, a peek row, or the card.
  const toastDrawn = toast ? toastForm(toast.value, hovering) : null;
  // The open gate was answered: the run's own row says so before Rust's event.
  const decideRun = useCallback(
    (confirmed: boolean) =>
      setAgent((r) => r && { ...r, steps: withDecision(r.steps, confirmed) }),
    [],
  );
  // The compact run panel's one way into the sections: the same run, in Agents › Voice.
  const openVoice = useCallback(() => {
    if (!notchRef.current) return;
    setBrowsing(true);
    takeFocus("agents", "voice");
  }, [takeFocus]);
  // A drill — a permission arriving, a dealt card clicked, a row picked —
  // is a coding agent, so the panel goes to where they live.
  useLayoutEffect(() => {
    if (selected) takeFocus("agents", "coding");
  }, [selected, takeFocus]);

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
  const populated =
    panelOpen ||
    toast !== null ||
    phase !== "idle" ||
    (resting && (shellShown || pillShown));
  // The pill reports its own measured box, so hovering it is hovering the
  // notch — the one writer of the click-box, where the old canvas strip and
  // the panel used to take turns at it.
  const panel = useNotchPanel(
    effectiveStyle,
    run,
    populated,
    dismissing || (pillLeaving && !hovering) || peekLeaving,
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
  // Home opened under the pointer holds the takeover gate (a banner waits)
  // without taking keyboard focus: that is `set_notch_peek`, not a panel.
  // After the effect above, so a click-in reaches Rust as "panel open" first.
  useEffect(() => {
    try {
      void commands.setNotchPeek(peeking);
    } catch {
      /* browser preview: no gate to tell */
    }
  }, [peeking]);
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
        // including from the agents surface and a toast, or the pill and the
        // panel fight over the same strip.
        setAgent(null);
        setToast(null);
        setWhisper(null);
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
    // Rust's own word that the notch went on or off. Off also closes a browse
    // still open, which nothing could otherwise reach once the shell is gone.
    const unlistenLabs = events.labsChanged.listen(({ payload }) => {
      setNotchOn(payload.notch);
      if (!payload.notch) {
        setBrowsing(false);
        setSelected(null);
        setWhisper(null);
      }
    });
    return () => {
      unlistenLabs.then((off) => off());
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
      // First, so anything else in the same command draws on a resting notch.
      if (command.rest) {
        clearInterval(typing);
        setAgent(null);
        setBrowsing(false);
        setSelected(null);
        setToast(null);
        setWhisper(null);
        setWaiting(0);
        setHovering(false);
        setPartial("");
        setDroppedDown(false);
        setDismissing(false);
        setPhase("idle");
      }
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
            "Said hi in #launch. The email to David is ready whenever you are.",
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
              result: { success: true, message: "no results", show: null, note: null, tidied: null },
            },
            {
              name: "GMAIL_FETCH_EMAILS",
              app: "Gmail",
              args: { query: "Genghis Khan" },
              requiresConfirmation: false,
              result: { success: true, message: "no results", show: null, note: null, tidied: null },
            },
            {
              name: "GMAIL_FETCH_EMAILS",
              app: "Gmail",
              args: { query: "from:Genghis" },
              requiresConfirmation: false,
              result: { success: true, message: "no results", show: null, note: null, tidied: null },
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
              preview: CONTACT_PREVIEW,
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
              result: { success: true, message: "no results", show: null, note: null, tidied: null },
            },
            {
              name: "GMAIL_FETCH_EMAILS",
              app: "Gmail",
              args: { query: "Charlie Yang" },
              requiresConfirmation: false,
              result: { success: true, message: "no results", show: null, note: null, tidied: null },
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
              result: { success: true, message: "3 results", show: null, note: null, tidied: null },
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
      if (command.card) setAgent(withIcon(CARD_PREVIEWS[command.card], command.icon));
      if (command.finder) setAgent(FINDER_PREVIEWS[command.finder]);
      if (command.toast) {
        const value = TOAST_PREVIEWS[command.toast];
        setToast({
          value: command.icon ? { ...value, icon: command.icon } : value,
          key: ++toastKey.current,
        });
        setAgent(null);
        setDismissing(false);
      }
      if (command.takeover) {
        // `stay`: the fixture keeps its place for a screenshot, not its dwell.
        const fixture: Island = { ...TAKEOVER_PREVIEWS[command.takeover] };
        if (command.level !== undefined) fixture.level = command.level;
        if (command.stay) fixture.dwellMs = fixture.tier === "card" ? null : 3_600_000;
        takeOver(fixture);
        setDismissing(false);
      }
      if (command.hover !== undefined) setHovering(command.hover);
      if (command.runs !== undefined) setRuns(command.runs ? VOICE_RUN_PREVIEWS : []);
      if (command.waiting !== undefined) setWaiting(command.waiting);
      if (command.islands !== undefined) setIslands(livePreviews(command.islands));
      if (command.open) {
        setBrowsing(true);
        navigate(command.open.section, command.open.sub);
      }
      if (command.palette !== undefined) setPalette(command.palette);
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
  }, [takeOver]);

  // An agent run. Its own listener: a separate event stream, a shared surface.
  useEffect(() => {
    const unlisten = events.agent.listen(({ payload }) => {
      switch (payload.kind) {
        case "runStarted":
          // A new run resets the panel; it is reused, not recreated.
          setAgent({ heard: payload.value, steps: [] });
          setToast(null);
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
        case "decided":
          // The countdown or a spoken reply answered the gate; its own buttons
          // already said so through `onDecide`.
          setAgent((r) => r && { ...r, steps: withDecision(r.steps, payload.value) });
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

  // Closes the panel: Escape, and the shell's ×.
  const closePanel = useCallback(() => {
    // A browse sets `PANEL_OPEN` in Rust (via `setPanelInteractive`), so
    // closing it has to clear that too — `closeBrowse` does, and clearing
    // only the webview `browsing` would leave the Rust flag stuck.
    // `invoke` throws synchronously in the browser preview; the webview side
    // of the close must still happen there.
    if (!agent) {
      try {
        void commands.closeBrowse();
      } catch {
        /* browser preview: no Tauri */
      }
      setBrowsing(false);
      setSelected(null);
      return;
    }
    // `closeAgent` also abandons a pending confirmation, so Escape reads as
    // "no" rather than leaving a write waiting on a channel nobody answers.
    try {
      void commands.closeAgent();
    } catch {
      // The preview has no Rust to send the dismiss back, so it closes here.
      setAgent(null);
    }
  }, [agent]);

  // Escape dismisses the panel. Guarded on `panelOpen` so a stray keystroke
  // cannot ask Rust to hide a pill mid-dictation.
  useEffect(() => {
    if (!panelOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closePanel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [panelOpen, closePanel]);

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
  //
  // A toast is torn down here too, after its collapse has played and not
  // before — then Rust is told, which hands the window back (to the listening
  // wings, or to rest). Told the other way round, a listening bar would swap
  // its style back under the toast mid-exit and cut it dead.
  useEffect(() => {
    if (!dismissing) return;
    const timer = setTimeout(() => {
      setDismissing(false);
      setPhase("idle");
      if (toastRef.current) {
        setToast(null);
        // The browse close is exactly the toast's: end the panel, nothing
        // more. `closeAgent` would also decline a gate a newer run has open.
        try {
          void commands.closeBrowse();
        } catch {
          /* browser preview: no Tauri */
        }
      }
    }, 600);
    return () => clearTimeout(timer);
  }, [dismissing]);

  // The toast leaving, by its clock, its × or its action: the panel collapses
  // into the notch on its close spring while the chip fades, then the timer
  // above takes it away. The queue hears it now, so the alert is not left
  // for its backstop.
  const leaveToast = useCallback(() => {
    setDismissing(true);
    const id = toastRef.current?.value.id;
    try {
      if (id) void commands.dismissIsland(id);
    } catch {
      /* browser preview: no queue to tell */
    }
  }, []);

  // An island's button, through the one action registry ⌘K also runs. An
  // action that opens the panel is the user choosing a place, so it is
  // remembered like a tab click.
  const actOnIsland = useCallback(
    (action: IslandAction) =>
      runAction(
        action.action,
        {
          open: (section, sub) => {
            if (!notchRef.current) return;
            setBrowsing(true);
            navigate(section, sub);
          },
        },
        action.args,
      ),
    [navigate],
  );

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
      data-exiting={dismissing || peekLeaving || undefined}
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
        className={`drop-pill drop-pill--${
          panelOpen
            ? "answering"
            : toast
              ? "toast"
              : resting
                ? pillShown && !hovering
                  ? "live"
                  : "resting"
                : phase
        }`}
        data-hover={resting && hovering ? "" : undefined}
        data-form={toastDrawn ?? undefined}
        ref={panel.surface}
        // Only ever reaches the DOM while the pointer is over the panel — Rust
        // hands the window clicks exactly then (`track_cursor`). A click on the
        // resting notch opens the sections panel whatever is running, and a
        // click inside Home's hover-open keeps it: this is the commit.
        onClick={() => {
          if (resting && notchOn) setBrowsing(true);
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
            shellShown
              ? " drop-pill__body--shell"
              : compact
                ? " drop-pill__body--ask"
                : toast
                  ? toastDrawn === "card"
                    ? " drop-pill__body--toast"
                    : toastDrawn === "peek"
                      ? " drop-pill__body--peek"
                      : " drop-pill__body--live"
                  : whisperOpen
                    ? " drop-pill__body--peek"
                    : resting && pillShown
                      ? " drop-pill__body--live"
                      : transcript
                        ? " drop-pill__body--transcript"
                        : ""
          }${dictating ? " drop-pill__body--cat" : ""}`}
          ref={panel.content}
        >
          {shellShown ? (
            // Everything under one notch: sections across the top, what was
            // said in Agents › Voice and the coding agents in Agents ›
            // Coding. Open for someone who asked for it, or in place under a
            // resting pointer (Home, see `shellState`).
            <NotchProvider
                value={{
                  agent,
                  setAgent,
                  sessions,
                  permissions,
                  islands,
                  runs,
                  removeRun,
                  act: actOnIsland,
                  selected,
                  select: setSelected,
                  resolvePermission,
                  showPeek,
                  bodyRef: answerRef,
                  navigate,
                  close: closePanel,
                  palette,
                  setPalette,
                }}
              >
                <NotchShell nav={nav} />
              </NotchProvider>
          ) : compact && agent ? (
            // A command shows itself, in place: the run and nothing else, the
            // way it did before the sections.
            <AgentRun
              key={agent.heard}
              run={agent}
              bodyRef={answerRef}
              // Agents › Voice is a section of the notch.
              onExpand={notchOn ? openVoice : undefined}
              onDecide={decideRun}
            />
          ) : resting ? (
            // At rest the panel is exactly the cutout and this body is empty —
            // unless islands are live, when it widens beside the camera to
            // show them (`IslandPill`).
            whisperOpen && whisper ? (
              <NotchToast
                key={whisper.key}
                island={whisper.value}
                hovering
                onLeave={endWhisper}
                onAction={actOnIsland}
                waiting={waiting}
              />
            ) : (
              pillShown && (
                <IslandPill
                  live={liveNow ? live : lastLive.current}
                  sessions={sessions}
                  permissions={permissions}
                  leaving={pillLeaving}
                  whisper={pillWhisper}
                  waiting={waiting}
                />
              )
            )
          ) : toast ? (
            <NotchToast
              key={toast.key}
              island={toast.value}
              hovering={hovering}
              onLeave={leaveToast}
              onAction={actOnIsland}
              side={whisper?.value}
              waiting={waiting}
            />
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
    </div>
    </CatSettingsProvider>
  );
}

/**
 * True while `on`, and for `ms` after it goes false — the surface outliving
 * its own exit. A returning `on` inside the window cancels the exit.
 */
function useLinger(on: boolean, ms: number): boolean {
  const [held, setHeld] = useState(on);
  useEffect(() => {
    if (on) {
      setHeld(true);
      return;
    }
    const timer = setTimeout(() => setHeld(false), ms);
    return () => clearTimeout(timer);
  }, [on, ms]);
  return on || held;
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

const mount = () =>
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <Overlay />
    </React.StrictMode>,
  );

// A plain browser (the tuner, screenshots) has no Rust: stand one in before
// anything calls `invoke`. A dynamic import, so the real app never loads it.
if ("__TAURI_INTERNALS__" in window) mount();
else void import("./lib/preview-backend").then((m) => (m.installPreviewBackend(), mount()));
