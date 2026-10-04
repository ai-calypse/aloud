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
import { NowOverview } from "./views/Now";

/**
 * Every section of the notch panel and every subsection in it, in tab order —
 * the whole map from docs/design/notch-sections.html, built or not. Only
 * subsections marked `ready` render, and a section with none is hidden (see
 * `readySections`), so this file is the plan and the panel is what has
 * landed. Adding a feature: write its view under `views/`, set `view`, flip
 * `ready`. The position here is the section's ⌘ number — append, never
 * reorder, or a learned shortcut moves.
 */
export const SECTIONS: Section[] = [
  {
    id: "now",
    name: "Now",
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
        view: NowOverview,
      },
      { id: "activity", name: "Activity", features: ["Shell activity", "Developer API"], ready: false },
    ],
  },
  {
    id: "agents",
    name: "Agents",
    icon: BotIcon,
    count: (d) => d.permissions.length,
    status: (d) => {
      if (d.permissions.length > 0) return `${d.permissions.length} waiting for you`;
      const busy = d.sessions.filter((s) => s.busy).length;
      if (busy > 0) return `${busy} running`;
      return d.agent && !d.agent.finished && !d.agent.error ? "Working on your command" : "";
    },
    subsections: [
      { id: "voice", name: "Voice", features: ["Voice commands", "Approvals"], ready: true, view: AgentsVoice },
      {
        id: "coding",
        name: "Coding",
        features: ["Claude Code", "Codex", "Agent approvals"],
        ready: true,
        view: AgentsCoding,
      },
      { id: "usage", name: "Usage", features: ["Plan usage", "AI usage tracker"], ready: true, view: AgentsUsage },
      { id: "prs", name: "Pull requests", features: ["GitHub PRs"], ready: false },
      { id: "terminal", name: "Terminal", features: ["Notch terminal"], ready: false },
    ],
  },
  {
    id: "media",
    name: "Media",
    icon: MusicIcon,
    subsections: [
      {
        id: "playing",
        name: "Now Playing",
        features: ["Now Playing", "Audio spectrum"],
        ready: true,
        view: MediaPlaying,
      },
      { id: "lyrics", name: "Lyrics", features: ["Synced lyrics"], ready: true, view: MediaLyrics },
      {
        id: "sound",
        name: "Sound",
        // No "Sound mixer": per-app volume needs Core Audio process taps.
        features: ["Audio switcher", "Mic mute"],
        ready: true,
        view: MediaSound,
      },
      { id: "devices", name: "Devices", features: ["AirPods", "Device battery"], ready: true, view: MediaDevices },
    ],
  },
  {
    id: "focus",
    name: "Focus",
    icon: TimerIcon,
    subsections: [
      { id: "timer", name: "Timer", features: ["Pomodoro", "Stopwatch"], ready: false },
      { id: "awake", name: "Keep Awake", features: ["Keep awake", "Focus mode"], ready: false },
      { id: "notes", name: "Notes", features: ["Quick notes"], ready: false },
      { id: "call", name: "Call prep", features: ["Teleprompter", "Camera mirror"], ready: false },
    ],
  },
  {
    id: "shelf",
    name: "Shelf",
    icon: InboxIcon,
    subsections: [
      { id: "files", name: "Files", features: ["File shelf", "AirDrop", "Image converter", "Zip"], ready: false },
      { id: "shots", name: "Screenshots", features: ["Screenshot shelf", "Screen capture"], ready: false },
      { id: "downloads", name: "Downloads", features: ["Download alerts"], ready: false },
      { id: "drives", name: "Drives", features: ["Drive eject"], ready: false },
    ],
  },
  {
    id: "clipboard",
    name: "Clipboard",
    icon: ClipboardIcon,
    subsections: [
      { id: "history", name: "History", features: ["Clipboard history", "Smart actions", "OCR"], ready: false },
      { id: "snippets", name: "Snippets", features: ["Text snippets"], ready: false },
      { id: "colors", name: "Colors", features: ["Color eyedropper"], ready: false },
    ],
  },
  {
    id: "today",
    name: "Today",
    icon: CalendarIcon,
    subsections: [
      { id: "calendar", name: "Calendar", features: ["Calendar"], ready: false },
      { id: "reminders", name: "Reminders", features: ["Reminders"], ready: false },
      { id: "weather", name: "Weather", features: ["Weather"], ready: false },
      { id: "stocks", name: "Stocks", features: ["Stocks ticker"], ready: false },
      { id: "inbox", name: "Inbox", features: ["Notification mirror"], ready: false },
    ],
  },
  {
    id: "mac",
    name: "Mac",
    icon: LaptopIcon,
    subsections: [
      { id: "stats", name: "Stats", features: ["System stats"], ready: false },
      { id: "power", name: "Power", features: ["Battery", "Full charge"], ready: false },
      { id: "privacy", name: "Privacy", features: ["Privacy indicator"], ready: false },
      { id: "windows", name: "Windows", features: ["Window snapping"], ready: false },
      { id: "display", name: "Display", features: ["Keyboard backlight", "External display"], ready: false },
      { id: "menubar", name: "Menu bar", features: ["Menu bar icon hiding", "Menu bar detach"], ready: false },
    ],
  },
  {
    id: "launch",
    name: "Launch",
    icon: LayoutGridIcon,
    subsections: [
      { id: "apps", name: "Apps", features: ["App launcher"], ready: false },
      { id: "shortcuts", name: "Shortcuts", features: ["Apple Shortcuts"], ready: false },
      { id: "search", name: "Search", features: ["Search and translate"], ready: false },
    ],
  },
];

/** What the panel shows: computed once, the registry never changes at runtime. */
export const READY = readySections(SECTIONS);
