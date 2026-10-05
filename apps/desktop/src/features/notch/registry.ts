import {
  ActivityIcon,
  BotIcon,
  CalendarIcon,
  ClipboardIcon,
  InboxIcon,
  LaptopIcon,
  LayoutGridIcon,
  MusicIcon,
  TimerIcon,
} from "lucide-react";
import { readySections, type Section } from "./sections";
import { AgentsCoding, AgentsUsage, AgentsVoice } from "./views/Agents";
import { MediaDevices } from "./views/MediaDevices";
import { MediaLyrics } from "./views/MediaLyrics";
import { MediaSound } from "./views/MediaSound";
import { MediaPlaying } from "./views/MediaPlaying";
import { ClipboardColors, ClipboardHistory, ClipboardSnippets } from "./views/Clipboard";
import { LaunchApps, LaunchSearch, LaunchShortcuts } from "./views/Launch";
import { MacPower } from "./views/MacPower";
import { MacPrivacy } from "./views/MacPrivacy";
import { MacStats } from "./views/MacStats";
import { MacWindows } from "./views/MacWindows";
import { FocusAwake, FocusNotes, FocusTimer } from "./views/Focus";
import { TodayCalendar, TodayReminders, TodayStocks, TodayWeather } from "./views/Today";
import { CalmHome } from "./views/CalmHome";
import { WidgetHome } from "./views/WidgetHome";
import { WidgetMediaDevices } from "./views/widgets/Devices";
import { WidgetMediaLyrics } from "./views/widgets/Lyrics";
import { WidgetMediaPlaying } from "./views/widgets/Playing";
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
import { WidgetCoding, WidgetUsage, WidgetVoice } from "./views/widgets/Agents";
import { WidgetApps, WidgetShortcuts } from "./views/widgets/Launch";
import { ShelfDownloads, ShelfDrives, ShelfFiles, ShelfShots } from "./views/Shelf";
import { WidgetShelfFiles } from "./views/widgets/ShelfFiles";
import { WidgetShelfShots } from "./views/widgets/ShelfShots";
import { WidgetShelfDownloads } from "./views/widgets/ShelfDownloads";
import { WidgetShelfDrives } from "./views/widgets/ShelfDrives";

/**
 * Every section of the notch panel and every subsection in it, in tab order —
 * the whole map from docs/design/notch-sections.html, built or not. Only
 * subsections marked `ready` render, and a section with none is hidden (see
 * `readySections`), so this file is the plan and the panel is what has
 * landed. Adding a feature: write its view under `views/`, set `view`, flip
 * `ready`. For the Widgets look give it a `widgetView` too (until then it
 * shows its calm `view` in either look). The position here is the section's ⌘ number — append, never
 * reorder, or a learned shortcut moves.
 */
export const SECTIONS: Section[] = [
  {
    id: "now",
    // The id stays "now": it is stored in nav and named by the overlay.
    name: "Home",
    blurb: "ranked by what is live, biggest first",
    icon: ActivityIcon,
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
        widgetView: WidgetHome,
      },
      { id: "activity", name: "Activity", features: ["Shell activity", "Developer API"], ready: false },
    ],
  },
  {
    id: "agents",
    name: "Agents",
    blurb: "Voice, coding, usage",
    icon: BotIcon,
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
        view: AgentsVoice,
        widgetView: WidgetVoice,
      },
      {
        id: "coding",
        name: "Coding",
        features: ["Claude Code", "Codex", "Agent approvals"],
        ready: true,
        view: AgentsCoding,
        widgetView: WidgetCoding,
      },
      {
        id: "usage",
        name: "Usage",
        features: ["Plan usage", "AI usage tracker"],
        ready: true,
        view: AgentsUsage,
        widgetView: WidgetUsage,
      },
      { id: "prs", name: "Pull requests", features: ["GitHub PRs"], ready: false },
      { id: "terminal", name: "Terminal", features: ["Notch terminal"], ready: false },
    ],
  },
  {
    id: "media",
    name: "Media",
    blurb: "Playing, lyrics, sound",
    icon: MusicIcon,
    subsections: [
      {
        id: "playing",
        name: "Now Playing",
        features: ["Now Playing", "Audio spectrum"],
        ready: true,
        view: MediaPlaying,
        widgetView: WidgetMediaPlaying,
      },
      {
        id: "lyrics",
        name: "Lyrics",
        features: ["Synced lyrics"],
        ready: true,
        view: MediaLyrics,
        widgetView: WidgetMediaLyrics,
      },
      {
        id: "sound",
        name: "Sound",
        // No "Sound mixer": per-app volume needs Core Audio process taps.
        features: ["Audio switcher", "Mic mute"],
        ready: true,
        view: MediaSound,
        widgetView: WidgetMediaSound,
      },
      {
        id: "devices",
        name: "Devices",
        features: ["AirPods", "Device battery"],
        ready: true,
        view: MediaDevices,
        widgetView: WidgetMediaDevices,
      },
    ],
  },
  {
    id: "focus",
    name: "Focus",
    blurb: "Timer, keep awake, notes",
    icon: TimerIcon,
    subsections: [
      {
        id: "timer",
        name: "Timer",
        features: ["Pomodoro", "Stopwatch"],
        ready: true,
        view: FocusTimer,
        widgetView: WidgetFocusTimer,
      },
      {
        id: "awake",
        name: "Keep Awake",
        features: ["Keep awake", "Focus mode"],
        ready: true,
        view: FocusAwake,
        widgetView: WidgetFocusAwake,
      },
      {
        id: "notes",
        name: "Notes",
        features: ["Quick notes"],
        ready: true,
        view: FocusNotes,
        widgetView: WidgetFocusNotes,
      },
      { id: "call", name: "Call prep", features: ["Teleprompter", "Camera mirror"], ready: false },
    ],
  },
  {
    id: "shelf",
    name: "Shelf",
    blurb: "Files in flight",
    icon: InboxIcon,
    subsections: [
      {
        id: "files",
        name: "Files",
        features: ["File shelf", "AirDrop", "Image converter", "Zip"],
        ready: true,
        view: ShelfFiles,
        widgetView: WidgetShelfFiles,
      },
      {
        id: "shots",
        name: "Screenshots",
        features: ["Screenshot shelf", "Screen capture"],
        ready: true,
        view: ShelfShots,
        widgetView: WidgetShelfShots,
      },
      {
        id: "downloads",
        name: "Downloads",
        features: ["Download alerts"],
        ready: true,
        view: ShelfDownloads,
        widgetView: WidgetShelfDownloads,
      },
      {
        id: "drives",
        name: "Drives",
        features: ["Drive eject"],
        ready: true,
        view: ShelfDrives,
        widgetView: WidgetShelfDrives,
      },
    ],
  },
  {
    id: "clipboard",
    name: "Clipboard",
    blurb: "Everything you copied",
    icon: ClipboardIcon,
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
    icon: CalendarIcon,
    subsections: [
      { id: "calendar", name: "Calendar", features: ["Calendar"], ready: true, view: TodayCalendar, widgetView: WidgetCalendar },
      { id: "reminders", name: "Reminders", features: ["Reminders"], ready: true, view: TodayReminders, widgetView: WidgetReminders },
      { id: "weather", name: "Weather", features: ["Weather"], ready: true, view: TodayWeather, widgetView: WidgetWeather },
      { id: "stocks", name: "Stocks", features: ["Stocks ticker"], ready: true, view: TodayStocks, widgetView: WidgetStocks },
      { id: "inbox", name: "Inbox", features: ["Notification mirror"], ready: false },
    ],
  },
  {
    id: "mac",
    name: "Mac",
    blurb: "The machine",
    icon: LaptopIcon,
    subsections: [
      {
        id: "stats",
        name: "Stats",
        features: ["System stats"],
        ready: true,
        view: MacStats,
        widgetView: WidgetMacStats,
      },
      {
        id: "power",
        name: "Power",
        features: ["Battery", "Full charge"],
        ready: true,
        view: MacPower,
        widgetView: WidgetMacPower,
      },
      {
        id: "privacy",
        name: "Privacy",
        features: ["Privacy indicator"],
        ready: true,
        view: MacPrivacy,
        widgetView: WidgetMacPrivacy,
      },
      {
        id: "windows",
        name: "Windows",
        features: ["Window snapping"],
        ready: true,
        view: MacWindows,
        widgetView: WidgetMacWindows,
      },
      { id: "display", name: "Display", features: ["Keyboard backlight", "External display"], ready: false },
      { id: "menubar", name: "Menu bar", features: ["Menu bar icon hiding", "Menu bar detach"], ready: false },
    ],
  },
  {
    id: "launch",
    name: "Launch",
    blurb: "Start anything",
    icon: LayoutGridIcon,
    subsections: [
      {
        id: "apps",
        name: "Apps",
        features: ["App launcher"],
        ready: true,
        view: LaunchApps,
        widgetView: WidgetApps,
      },
      {
        id: "shortcuts",
        name: "Shortcuts",
        features: ["Apple Shortcuts"],
        ready: true,
        view: LaunchShortcuts,
        widgetView: WidgetShortcuts,
      },
      {
        id: "search",
        name: "Search",
        features: ["Search apps, notes and files"],
        ready: true,
        // No widgetView: its hits are four kinds of row (recent and found apps,
        // notes, files) under an Enter-opens-the-first-hit box, which Siri
        // Suggestions' four fixed rows cannot hold without hiding most of them.
        view: LaunchSearch,
      },
    ],
  },
];

/** What the panel shows: computed once, the registry never changes at runtime. */
export const READY = readySections(SECTIONS);
