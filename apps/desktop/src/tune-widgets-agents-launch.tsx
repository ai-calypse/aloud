import { useState, type ReactNode } from "react";
// The cards and rows the widgets frame (approvals, previews, Finder results) are styled here.
import "./styles/overlay.css";
import type {
  AgentPermission,
  AgentSession,
  AgentTranscript,
  InstalledApp,
  LauncherPrefs,
  Quota,
} from "./lib/bindings";
// The cards and rows the widgets frame (approvals, previews, Finder results) are styled here.
import "./styles/overlay.css";
import type { Run } from "./lib/agent-steps";
import { AGENT_PREVIEW, CARD_PREVIEWS, FINDER_PREVIEWS } from "./features/overlay/previews";
import { usageBars, extraUsage } from "./features/notch/views/widgets/agents-logic";
import {
  CodingDetail,
  CodingList,
  UsageBoard,
  VoiceRun,
} from "./features/notch/views/widgets/Agents";
import { AppsBoard, ShortcutsBoard } from "./features/notch/views/widgets/Launch";

/**
 * Fixtures for the widgets look's Agents and Launch views: real-shaped data
 * through the same presentational components the notch uses, in the states a
 * Mac only reaches by holding a key, running agents or waiting for a plan
 * limit. Labelled as fixtures; the app never shows them.
 */

const noop = () => {};
const MIN = 60_000;
const ago = (ms: number) => new Date(Date.now() - ms).toISOString();
const later = (ms: number) => new Date(Date.now() + ms).toISOString();

const done = { success: true, message: "done", show: null, note: null, tidied: null };

const RUN_DONE: Run = {
  ...AGENT_PREVIEW,
  steps: [
    { ...AGENT_PREVIEW.steps[0] },
    { ...AGENT_PREVIEW.steps[1], confirmed: true, result: done },
  ],
  finished: "Said hi in #launch and emailed David the project update.",
};

const RUN_WORKING: Run = {
  heard: "Move my 3pm to Thursday and tell the team",
  steps: [
    { ...AGENT_PREVIEW.steps[0], confirmed: true, result: done },
    { ...CARD_PREVIEWS.email.steps[0], requiresConfirmation: false, preview: null },
  ],
  planned: [{ name: "SLACK_CHAT_POST_MESSAGE", app: "Slack" }],
  status: "Rate limited, retrying…",
};

const RUN_FAILED: Run = {
  heard: "Email David the project update",
  steps: [
    {
      ...CARD_PREVIEWS.email.steps[0],
      confirmed: true,
      result: { ...done, success: false, message: "Gmail is not connected" },
    },
  ],
  error: "Gmail is not connected. Connect it in Settings and ask again.",
};

const RUN_QUESTION: Run = {
  heard: "Text Sam that I am running late",
  steps: [],
  question: { question: "Which Sam?", options: ["Sam Lee", "Sam Ortiz", "Sam Okafor"] },
};

const RUN_FINDER: Run = FINDER_PREVIEWS.list;

const SESSIONS: AgentSession[] = [
  {
    id: "s1",
    agent: "claude",
    title: "Port the hotkey reducer",
    prompt: "Run the workspace tests and fix whatever breaks",
    cwd: "/Users/dev/aloud-tauri",
    tool: "Bash: cargo test --workspace",
    model: "opus-4-8",
    branch: "feat/notch-widgets",
    lastActivity: ago(40_000),
    subagents: [
      { label: "Explore: find the gate", model: "haiku", tool: "Grep", busy: true },
      { label: "Review the diff", model: "sonnet", tool: null, busy: false },
    ],
    busy: true,
    transient: null,
  },
  {
    id: "s2",
    agent: "claude",
    title: "Docs sweep",
    prompt: "Update the README for the new look",
    cwd: "/Users/dev/website",
    tool: "Bash: git push origin main",
    model: "sonnet-5",
    branch: "main",
    lastActivity: ago(2 * MIN),
    subagents: [],
    busy: true,
    transient: null,
  },
  {
    id: "s3",
    agent: "codex",
    title: "Migrate settings",
    prompt: null,
    cwd: "/Users/dev/store",
    tool: null,
    model: "gpt-5.6-sol",
    branch: null,
    lastActivity: ago(18 * MIN),
    subagents: [],
    busy: false,
    transient: null,
  },
];

const PERMISSION: AgentPermission = {
  id: "s2",
  sessionId: "s2",
  agent: "claude",
  tool: "Bash",
  input: { command: "git push origin main" },
  cwd: "/Users/dev/website",
  questions: [],
  open: true,
  danger: false,
};

const TRANSCRIPT: AgentTranscript = {
  entries: [
    { kind: "user", tool: null, text: "Run the workspace tests and fix whatever breaks", result: null, isError: false },
    { kind: "toolUse", tool: "Read", text: "Reading reducer.rs", result: "ok", isError: false },
    { kind: "toolUse", tool: "Read", text: "Reading tests.rs", result: "ok", isError: false },
    { kind: "toolUse", tool: "Edit", text: "Editing reducer.rs", result: "ok", isError: false },
    { kind: "toolUse", tool: "Bash", text: "cargo test --workspace", result: "2 failed", isError: true },
    {
      kind: "assistant",
      tool: null,
      text: "Two tests fail because the reducer drops a key-up that arrives before its key-down. I am adding the missing guard and re-running.",
      result: null,
      isError: false,
    },
  ],
  inputTokens: 18_400,
  outputTokens: 5_200,
  cacheReadTokens: 410_000,
  costUsd: 1.42,
};

const QUOTA: Quota = {
  fiveHour: { percent: 42, resetsAt: later(2 * 60 * MIN + 10 * MIN) },
  sevenDay: { percent: 31, resetsAt: later(3 * 24 * 60 * MIN) },
  sevenDayOpus: { percent: 91, resetsAt: later(3 * 24 * 60 * MIN) },
  extraUsagePercent: 12,
};

const APP_NAMES = [
  "Arc", "Calendar", "Claude", "Figma", "Finder", "Ghostty", "Mail", "Messages", "Notes",
  "Notion", "Safari", "Slack", "Spotify", "Xcode", "Zed", "Preview", "Photos", "Music", "Maps", "Terminal",
];
const APPS: InstalledApp[] = APP_NAMES.map((name) => ({ name, bundle_id: null, icon: null }));
const PREFS: LauncherPrefs = { pins: ["Slack", "Figma", "Notes", "Xcode", "Spotify"], recents: ["Arc", "Claude", "Mail", "Safari"] };
const split = (apps: InstalledApp[], prefs: LauncherPrefs) => {
  const byName = new Map(apps.map((a) => [a.name, a]));
  const pinned = prefs.pins.flatMap((n) => byName.get(n) ?? []);
  const recent = prefs.recents.flatMap((n) => byName.get(n) ?? []);
  const taken = new Set([...prefs.pins, ...prefs.recents]);
  return { pinned, recent, rest: apps.filter((a) => !taken.has(a.name)) };
};

const SHORTCUTS = [
  "Block Off an Hour", "Take a Break", "Make GIF", "New Note with Date",
  "Text Running Late", "Email Last Image", "Play Swimming", "Open YouTube", "Log Water", "Start Standup",
];

function Block({ name, note, children }: { name: string; note: string; children: ReactNode }) {
  return (
    <section>
      <h2 className="m-0 text-notch-body font-semibold text-notch-ink">{name}</h2>
      <p className="m-0 mb-3 text-notch-meta text-notch-ink-3">{note}</p>
      {children}
    </section>
  );
}

export function AgentsLaunchGallery() {
  const [query, setQuery] = useState("");
  const grouped = split(APPS, PREFS);
  const matches = APPS.filter((a) => a.name.toLowerCase().includes("s"));
  return (
    <div data-notch-style="widgets" className="grid w-[780px] gap-9 font-notch-w">
      <Block name="Agents › Voice · at an approval" note="the run's steps and the gate it is waiting on; the gate is the app's own card in a tile">
        <VoiceRun run={AGENT_PREVIEW} onAddTab={noop} onDecide={noop} />
      </Block>
      <Block name="Agents › Voice · working" note="a step queued, a transient status line, a Finder result under its step">
        <VoiceRun run={RUN_WORKING} onAddTab={noop} />
        <div className="h-9" />
        <VoiceRun run={RUN_FINDER} onAddTab={noop} />
      </Block>
      <Block name="Agents › Voice · done, failed, asking" note="the answer, an error in words, and the model's question with options">
        <VoiceRun run={RUN_DONE} onAddTab={noop} />
        <div className="h-9" />
        <VoiceRun run={RUN_FAILED} />
        <div className="h-9" />
        <VoiceRun run={RUN_QUESTION} />
      </Block>

      <Block name="Agents › Coding · a session each" note="busy, asking permission (its card first) and idle">
        <CodingList sessions={SESSIONS} permissions={[PERMISSION]} onSelect={noop} onResolve={noop} onPeek={noop} />
      </Block>
      <Block name="Agents › Coding · empty" note="no sessions: the calm view's words">
        <CodingList sessions={[]} permissions={[]} onSelect={noop} onResolve={noop} onPeek={noop} />
      </Block>
      <Block name="Agents › Coding · one session" note="the latest message and tool calls, subagents, usage, the approval if it is asking">
        <CodingDetail
          session={SESSIONS[0]}
          permission={null}
          queued={0}
          onResolve={noop}
          onBack={noop}
          onPeek={noop}
          transcript={TRANSCRIPT}
        />
        <div className="h-9" />
        <CodingDetail
          session={SESSIONS[1]}
          permission={PERMISSION}
          queued={1}
          onResolve={noop}
          onBack={noop}
          onPeek={noop}
          transcript={{ ...TRANSCRIPT, entries: [] }}
        />
      </Block>

      <Block name="Agents › Usage · plan read" note="Screen Time: blue, cyan and orange bars, one per window, the shortest as the big number; extra usage as a fourth column">
        <UsageBoard idle={false} error={null} loaded bars={usageBars(QUOTA)} extra={extraUsage(QUOTA)} onStart={noop} />
      </Block>
      <Block name="Agents › Usage · not asked yet, and no data" note="the opt-in before macOS asks for the Keychain, and a plan with no windows">
        <UsageBoard idle error="Not signed in to Claude Code" loaded={false} bars={[]} extra={null} onStart={noop} />
        <div className="h-9" />
        <UsageBoard idle={false} error={null} loaded bars={[]} extra={null} onStart={noop} />
      </Block>

      <Block name="Launch › Apps" note="pinned (App Store blue), recent (Siri Suggestions), every app in the Large tile; hover an icon for its pin">
        <AppsBoard
          searching={false}
          matches={[]}
          {...grouped}
          prefs={PREFS}
          onOpen={noop}
          onPin={noop}
        />
      </Block>
      <Block name="Launch › Apps · filtering, and nothing pinned" note="the matches with the top hit Enter opens; no pins or recents leaves the list centred">
        <AppsBoard searching matches={matches} pinned={[]} recent={[]} rest={[]} prefs={PREFS} onOpen={noop} onPin={noop} />
        <div className="h-9" />
        <AppsBoard searching matches={[]} pinned={[]} recent={[]} rest={[]} prefs={PREFS} onOpen={noop} onPin={noop} />
        <div className="h-9" />
        <AppsBoard
          searching={false}
          matches={[]}
          pinned={[]}
          recent={[]}
          rest={APPS}
          prefs={{ pins: [], recents: [] }}
          onOpen={noop}
          onPin={noop}
        />
      </Block>

      <Block name="Launch › Shortcuts" note="four to a Medium tile; one running, one done, one failed (its message is read under the board)">
        <ShortcutsBoard
          names={SHORTCUTS}
          shown={SHORTCUTS.filter((n) => n.toLowerCase().includes(query.toLowerCase()))}
          runs={{
            "Take a Break": { state: "running" },
            "Make GIF": { state: "done" },
            "Text Running Late": { state: "failed", message: "Messages is not allowed to be automated" },
          }}
          query={query}
          onQuery={setQuery}
          onRun={noop}
        />
      </Block>
      <Block name="Launch › Shortcuts · empty" note="no library yet: the calm view's words and its one action">
        <ShortcutsBoard names={[]} shown={[]} runs={{}} query="" onQuery={noop} onRun={noop} />
      </Block>
    </div>
  );
}
