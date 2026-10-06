import { readySections, type Section } from "./sections";
import { AgentsCoding, AgentsUsage } from "./views/Agents";
import { MediaLyrics } from "./views/MediaLyrics";
import { MediaPlaying } from "./views/MediaPlaying";
import { ClipboardColors, ClipboardHistory, ClipboardSnippets } from "./views/Clipboard";
import { LaunchSearch } from "./views/Launch";
import { CalmHome } from "./views/CalmHome";
import { ShelfFiles, ShelfShots } from "./views/Shelf";
import { WidgetMediaDevices } from "./views/widgets/Devices";
import { WidgetMediaSound } from "./views/widgets/Sound";
import { WidgetCalendar } from "./views/widgets/Calendar";
import { WidgetReminders } from "./views/widgets/Reminders";
import { WidgetStocks } from "./views/widgets/Stocks";
import { WidgetWeather } from "./views/widgets/Weather";
import { WidgetFocusAwake, WidgetFocusNotes, WidgetFocusTimer } from "./views/widgets/Focus";
import {
  WidgetMacPower,
  WidgetMacPrivacy,
  WidgetMacStats,
  WidgetMacWindows,
} from "./views/widgets/Mac";
import { WidgetVoice } from "./views/widgets/Agents";
import { WidgetApps, WidgetShortcuts } from "./views/widgets/Launch";
import { WidgetShelfDownloads } from "./views/widgets/ShelfDownloads";
import { WidgetShelfDrives } from "./views/widgets/ShelfDrives";

/**
 * Every section of the notch panel and every subsection in it, in tab order —
 * the whole map from docs/design/notch-sections.html, built or not. Only
 * subsections marked `ready` render, and a section with none is hidden (see
 * `readySections`), so this file is the plan and the panel is what has
 * landed. Adding a feature: write its view, set `view`, flip `ready`. A view
 * is a list under `views/` or a widget board under `views/widgets/`, whichever
 * the section was drawn as (DESIGN.md, Widget boards). The position here is
 * the section's ⌘ number — append, never reorder, or a learned shortcut moves.
 */
export const SECTIONS: Section[] = [
  {
    id: "now",
    // The id stays "now": it is stored in nav and named by the overlay.
    name: "Home",
    blurb: "ranked by what is live, biggest first",
    status: (d) => {
      const live = d.islands.filter((i) => i.kind === "live").length;
      return live > 0 ? `${live} live` : "";
    },
    subsections: [
      {
        id: "overview",
        name: "Overview",
        features: ["Live activities"],
        ready: true,
        view: CalmHome,
      },
      { id: "activity", name: "Activity", features: ["Shell activity", "Developer API"], ready: false },
    ],
  },
  {
    id: "agents",
    name: "Agents",
    blurb: "Voice, coding, usage",
    count: (d) => d.permissions.length,
    status: (d) => {
      if (d.permissions.length > 0) return `${d.permissions.length} waiting for you`;
      const busy = d.sessions.filter((s) => s.busy).length;
      if (busy > 0) return `${busy} running`;
      return d.agent && !d.agent.finished && !d.agent.error ? "Working on your command" : "";
    },
    subsections: [
      {
        id: "voice",
        name: "Voice",
        features: ["Voice commands", "Approvals"],
        ready: true,
        view: WidgetVoice,
      },
      {
        id: "coding",
        name: "Coding",
        features: ["Claude Code", "Codex", "Agent approvals"],
        ready: true,
        view: AgentsCoding,
      },
      {
        id: "usage",
        name: "Usage",
        features: ["Plan usage", "AI usage tracker"],
        ready: true,
        view: AgentsUsage,
      },
      { id: "prs", name: "Pull requests", features: ["GitHub PRs"], ready: false },
      { id: "terminal", name: "Terminal", features: ["Notch terminal"], ready: false },
    ],
  },
  {
    id: "media",
    name: "Media",
    blurb: "Playing, lyrics, sound",
    subsections: [
      {
        id: "playing",
        name: "Now Playing",
        features: ["Now Playing", "Audio spectrum"],
        ready: true,
        view: MediaPlaying,
      },
      {
        id: "lyrics",
        name: "Lyrics",
        features: ["Synced lyrics"],
        ready: true,
        view: MediaLyrics,
      },
      {
        id: "sound",
        name: "Sound",
        // No "Sound mixer": per-app volume needs Core Audio process taps.
        features: ["Audio switcher", "Mic mute"],
        ready: true,
        view: WidgetMediaSound,
      },
      {
        id: "devices",
        name: "Devices",
        features: ["AirPods", "Device battery"],
        ready: true,
        view: WidgetMediaDevices,
      },
    ],
  },
  {
    id: "focus",
    name: "Focus",
    blurb: "Timer, keep awake, notes",
    subsections: [
      {
        id: "timer",
        name: "Timer",
        features: ["Pomodoro", "Stopwatch"],
        ready: true,
        view: WidgetFocusTimer,
      },
      {
        id: "awake",
        name: "Keep Awake",
        features: ["Keep awake", "Focus mode"],
        ready: true,
        view: WidgetFocusAwake,
      },
      {
        id: "notes",
        name: "Notes",
        features: ["Quick notes"],
        ready: true,
        view: WidgetFocusNotes,
      },
      { id: "call", name: "Call prep", features: ["Teleprompter", "Camera mirror"], ready: false },
    ],
  },
  {
    id: "shelf",
    name: "Shelf",
    blurb: "Files in flight",
    subsections: [
      {
        id: "files",
        name: "Files",
        features: ["File shelf", "AirDrop", "Image converter", "Zip"],
        ready: true,
        view: ShelfFiles,
      },
      {
        id: "shots",
        name: "Screenshots",
        features: ["Screenshot shelf", "Screen capture"],
        ready: true,
        view: ShelfShots,
      },
      {
        id: "downloads",
        name: "Downloads",
        features: ["Download alerts"],
        ready: true,
        view: WidgetShelfDownloads,
      },
      {
        id: "drives",
        name: "Drives",
        features: ["Drive eject"],
        ready: true,
        view: WidgetShelfDrives,
      },
    ],
  },
  {
    id: "clipboard",
    name: "Clipboard",
    blurb: "Everything you copied",
    subsections: [
      {
        id: "history",
        name: "History",
        features: ["Clipboard history", "Smart actions", "OCR"],
        ready: true,
        view: ClipboardHistory,
      },
      {
        id: "snippets",
        name: "Snippets",
        features: ["Text snippets"],
        ready: true,
        view: ClipboardSnippets,
      },
      {
        id: "colors",
        name: "Colors",
        features: ["Color eyedropper"],
        ready: true,
        view: ClipboardColors,
      },
    ],
  },
  {
    id: "today",
    name: "Today",
    blurb: "Calendar, reminders, weather",
    subsections: [
      { id: "calendar", name: "Calendar", features: ["Calendar"], ready: true, view: WidgetCalendar },
      { id: "reminders", name: "Reminders", features: ["Reminders"], ready: true, view: WidgetReminders },
      { id: "weather", name: "Weather", features: ["Weather"], ready: true, view: WidgetWeather },
      { id: "stocks", name: "Stocks", features: ["Stocks ticker"], ready: true, view: WidgetStocks },
      { id: "inbox", name: "Inbox", features: ["Notification mirror"], ready: false },
    ],
  },
  {
    id: "mac",
    name: "Mac",
    blurb: "The machine",
    subsections: [
      {
        id: "stats",
        name: "Stats",
        features: ["System stats"],
        ready: true,
        view: WidgetMacStats,
      },
      {
        id: "power",
        name: "Power",
        features: ["Battery", "Full charge"],
        ready: true,
        view: WidgetMacPower,
      },
      {
        id: "privacy",
        name: "Privacy",
        features: ["Privacy indicator"],
        ready: true,
        view: WidgetMacPrivacy,
      },
      {
        id: "windows",
        name: "Windows",
        features: ["Window snapping"],
        ready: true,
        view: WidgetMacWindows,
      },
      { id: "display", name: "Display", features: ["Keyboard backlight", "External display"], ready: false },
      { id: "menubar", name: "Menu bar", features: ["Menu bar icon hiding", "Menu bar detach"], ready: false },
    ],
  },
  {
    id: "launch",
    name: "Launch",
    blurb: "Start anything",
    subsections: [
      {
        id: "apps",
        name: "Apps",
        features: ["App launcher"],
        ready: true,
        view: WidgetApps,
      },
      {
        id: "shortcuts",
        name: "Shortcuts",
        features: ["Apple Shortcuts"],
        ready: true,
        view: WidgetShortcuts,
      },
      {
        id: "search",
        name: "Search",
        features: ["Search apps, notes and files"],
        ready: true,
        // A list, not a board: its hits are four kinds of row (recent and
        // found apps, notes, files) under an Enter-opens-the-first-hit box,
        // which Siri Suggestions' four fixed rows cannot hold.
        view: LaunchSearch,
      },
    ],
  },
];

/** What the panel shows: computed once, the registry never changes at runtime. */
export const READY = readySections(SECTIONS);
