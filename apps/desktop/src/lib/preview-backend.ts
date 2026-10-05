import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  Battery,
  BtDevice,
  ClipboardEntry,
  Event as CalendarEvent,
  FocusSnapshot,
  FocusStats,
  InstalledApp,
  Island,
  LaunchFile,
  LauncherPrefs,
  Lyrics,
  Note,
  NowPlaying,
  PickedColor,
  PrivacyUse,
  Quota,
  Reading,
  Reminder,
  ScreenShot,
  Settings,
  ShelfDownload,
  ShelfDrive,
  ShelfItem,
  Snippet,
  SoundState,
  StatsView,
  SystemStats,
  Ticker,
  Weather,
} from "./bindings";
import { VOICE_RUN_PREVIEWS, livePreviews } from "@/features/overlay/previews";

/**
 * A stand-in for Rust, for opening the overlay in a plain browser (the tuner,
 * screenshots, design review). `overlay.tsx` imports it dynamically and only
 * when `__TAURI_INTERNALS__` is absent, so the real app never loads it. Every
 * name, track, file and number below is made up. Commands that only act
 * (play, eject, paste…) resolve to null; anything unknown resolves to null
 * and is logged once, so a missing fixture shows up in the console, not as a
 * silent empty section.
 *
 * ponytail: read-only — a press does not change the next reading. State that
 * follows clicks is the upgrade if the preview ever needs to be interactive.
 */

const MIN = 60_000;
const HOUR = 3_600_000;
const T0 = Date.now();
const SEC = Math.floor(T0 / 1000);
const params = new URLSearchParams(location.search);

/** A 64px picture standing in for a thumbnail or a cover: a gradient with a window-ish block. */
function art(hue: number, size = 64): string {
  const a = `hsl(${hue} 70% 58%)`;
  const b = `hsl(${(hue + 55) % 360} 65% 38%)`;
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="${size}" height="${size}">` +
    `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${a}"/>` +
    `<stop offset="1" stop-color="${b}"/></linearGradient></defs><rect width="64" height="64" fill="url(#g)"/>` +
    `<rect x="8" y="12" width="48" height="40" rx="4" fill="#fff" fill-opacity=".8"/>` +
    `<rect x="8" y="12" width="48" height="8" rx="4" fill="#000" fill-opacity=".25"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

/** A square album cover: two discs on a gradient. */
function cover(hue: number): string {
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120"><defs>` +
    `<linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${hue} 80% 55%)"/>` +
    `<stop offset="1" stop-color="hsl(${(hue + 70) % 360} 70% 22%)"/></linearGradient></defs>` +
    `<rect width="120" height="120" fill="url(#g)"/><circle cx="78" cy="46" r="26" fill="#fff" fill-opacity=".18"/>` +
    `<circle cx="42" cy="84" r="34" fill="#000" fill-opacity=".22"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

/** An app tile: a rounded square in the app's colour with its initial. */
function appIcon(name: string): string {
  const hue = [...name].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">` +
    `<stop offset="0" stop-color="hsl(${hue} 70% 62%)"/><stop offset="1" stop-color="hsl(${hue} 70% 42%)"/></linearGradient></defs>` +
    `<rect width="64" height="64" rx="14" fill="url(#g)"/><text x="32" y="43" text-anchor="middle" ` +
    `font-family="system-ui,sans-serif" font-size="30" font-weight="600" fill="#fff">${name[0]}</text></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

// ── settings ─────────────────────────────────────────────────────────────

const SETTINGS = (): Settings => ({
  catImages: true,
  catImagesDictation: true,
  catImagesAgents: true,
  onboarded: true,
  overlayStyle: "dropPill",
  hotkeys: {
    hold: { kind: "modifierKey", key: "fn", modifiers: [] },
    toggle: null,
    copy_again: null,
    ask: null,
    rewrite: null,
    meeting_notes: null,
    agent: null,
  },
});

const STATS: StatsView = {
  timeSavedSeconds: 7_800,
  totalWords: 48_210,
  totalSessions: 912,
  currentStreakDays: 9,
  bestStreakDays: 21,
  lastDictationDay: null,
};

// ── media ────────────────────────────────────────────────────────────────

const SONG: NowPlaying = {
  player: "spotify",
  source: { app: "Spotify", bundleId: "com.spotify.client", host: null, tab: null },
  state: "playing",
  title: "Night Drive",
  artist: "Halcyon",
  album: "Neon",
  durationMs: 200_000,
  positionMs: 0,
  speedPct: 100,
  artworkUrl: null,
  song: null,
};
/** The track is mid-song when the page loads, and keeps moving. */
const SONG_START = 64_000;

const LINES = [
  "Headlights fold the dark in two",
  "Nothing on the radio but you",
  "",
  "I count the lamps along the bay",
  "Every one a mile away",
  "We were never going anywhere",
  "That was always the way",
  "Turn it up, turn it up",
  "The city is a quiet song",
  "Hold the wheel and hold the light",
  "Nothing here belongs to night",
].map((text, i) => ({ ms: 20_000 + i * 11_000, text }));
const LYRICS: Lyrics = { lines: LINES, plain: null, instrumental: false };

const reading = (): Reading => ({
  now: { ...SONG, positionMs: (SONG_START + (Date.now() - T0)) % SONG.durationMs },
  denied: null,
  browsers: [],
});

const SOUND: SoundState = {
  outputs: [
    { id: 1, name: "MacBook Pro Speakers", transport: "Built-in", isDefault: false },
    { id: 2, name: "AirPods Pro", transport: "Bluetooth", isDefault: true },
    { id: 3, name: "LG UltraFine", transport: "HDMI", isDefault: false },
  ],
  inputs: [
    { id: 4, name: "MacBook Pro Microphone", transport: "Built-in", isDefault: true },
    { id: 5, name: "AirPods Pro", transport: "Bluetooth", isDefault: false },
  ],
  micMuted: false,
};

const bt = (name: string, kind: string, percent: number[] | null, labels: string[] = []): BtDevice => ({
  name,
  address: name.toLowerCase().replace(/\W/g, ":"),
  kind,
  connected: percent !== null,
  batteries: (percent ?? []).map((p, i) => ({ label: labels[i] ?? "", percent: p })),
});
const DEVICES: BtDevice[] = [
  bt("AirPods Pro", "headphones", [100, 98, 63], ["Left", "Right", "Case"]),
  bt("Magic Mouse", "mouse", [80]),
  bt("Magic Keyboard", "keyboard", [18]),
  bt("Sony WH-1000XM5", "headphones", null),
  bt("Magic Trackpad", "trackpad", []),
];

// ── focus ────────────────────────────────────────────────────────────────

const focus = (): FocusSnapshot => ({
  timer: {
    mode: "pomodoro",
    phase: "focus",
    status: "running",
    round: 2,
    rounds: 4,
    preset: { focus: 25, rest: 5 },
    at: Date.now() + 18 * MIN + 42_000,
    heldMs: 0,
    totalMs: 25 * MIN,
  },
  awake: { active: true, minutes: 120, until: Date.now() + 97 * MIN },
});

const isoDay = (daysAgo: number) => {
  const d = new Date(Date.now() - daysAgo * 86_400_000);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;
};
const FOCUS_STATS: FocusStats = {
  streak: 5,
  week: [25, 50, 0, 75, 25, 100, 50].map((minutes, i) => ({
    day: isoDay(6 - i),
    sessions: minutes / 25,
    minutes,
  })),
};

const NOTES: Note[] = [
  { path: "/demo/notes/call-prep.md", title: "Call prep: Maya", date: SEC - 3600, content: "Call prep: Maya\nOpen with the usage numbers, then the roadmap." },
  { path: "/demo/notes/groceries.md", title: "Grocery list", date: SEC - 86_400, content: "Grocery list\nOats, coffee, lemons, eggs" },
  { path: "/demo/notes/notch-ideas.md", title: "Notch ideas", date: SEC - 9 * 86_400, content: "# Notch ideas\nWidgets are an option, not the default" },
];

// The one thing here that follows a click: a copy into Apple Notes, so the
// "In Notes" and "Syncing" states can be seen. `?notesSync=denied` shows the
// permission failure instead.
const NOTE_MARKS: Record<string, "new" | "synced" | "edited"> = {
  [NOTES[0].path]: "synced",
  [NOTES[1].path]: "edited",
  [NOTES[2].path]: "new",
};

// ── shelf ────────────────────────────────────────────────────────────────

const FILE_NAMES: [string, boolean, "file" | "folder"][] = [
  ["report-q3.pdf", false, "file"],
  ["demo.mov", false, "file"],
  ["icons.zip", false, "file"],
  ["mock.png", true, "file"],
  ["notes.md", false, "file"],
  ["Screen 4.png", true, "file"],
  ["invoice.pdf", false, "file"],
  ["Assets", false, "folder"],
  ["deck.key", false, "file"],
  ["photo.heic", true, "file"],
];
const SHELF: ShelfItem[] = FILE_NAMES.map(([name, image, kind], i) => ({
  id: `f${i}`,
  name,
  path: `/Users/demo/Shelf/${name}`,
  original: `/Users/demo/Desktop/${name}`,
  kind,
  size: 400_000 + i * 730_000,
  added: SEC - i * 600,
  image,
}));

const SHOTS: ScreenShot[] = Array.from({ length: 7 }, (_, i) => ({
  name: `Screenshot 2026-10-04 at 10.${40 - i}.12.png`,
  path: `/Users/demo/Desktop/shots/${i}.png`,
  taken: SEC - 30 - i * 1900,
}));

const DOWNLOADS: ShelfDownload[] = (
  [
    ["Aloud-0.9.2.dmg", 84_000_000, 120],
    ["wireframes.png", 1_900_000, 3_100],
    ["Q3 board deck.pdf", 6_400_000, 9_000],
    ["dataset.csv", 31_000_000, 40_000],
    ["Invoice 4411.pdf", 180_000, 90_000],
    ["photo-from-maya.heic", 2_800_000, 200_000],
  ] as [string, number, number][]
).map(([name, size, ago]) => ({ name, path: `/Users/demo/Downloads/${name}`, size, finished: SEC - ago }));

const DRIVES: ShelfDrive[] = [
  { name: "SANDISK 64", mount: "/Volumes/SANDISK 64", bus: "USB", size: 64_000_000_000, free: 41_000_000_000 },
  { name: "Photos SD", mount: "/Volumes/Photos SD", bus: "Secure Digital", size: 128_000_000_000, free: 21_000_000_000 },
  { name: "Backup", mount: "/Volumes/Backup", bus: "USB", size: 1_000_000_000_000, free: 22_000_000_000 },
];

// ── clipboard ────────────────────────────────────────────────────────────

const clip = (
  id: number,
  ago: number,
  kind: ClipboardEntry["kind"],
  preview: string,
  extra: Partial<ClipboardEntry> = {},
): ClipboardEntry => ({
  id,
  kind,
  preview,
  thumbnail: null,
  pinned: false,
  copiedAt: SEC - ago,
  actions: kind === "text" ? [{ id: "copy", label: "Copy" }] : [],
  ...extra,
});
const CLIPS: ClipboardEntry[] = [
  clip(1, 20, "text", "cargo test --workspace -- --nocapture", { pinned: true }),
  clip(2, 140, "text", "https://example.com/launch-notes"),
  clip(3, 600, "image", "Image 1280×720", { thumbnail: art(210) }),
  clip(4, 1_500, "text", "maya@example.com", { actions: [{ id: "mailto", label: "Compose" }] }),
  clip(5, 3_600, "files", "report-q3.pdf, invoice.pdf"),
  clip(6, 7_200, "text", "Ship the notch preview mocks before Friday's review."),
  clip(7, 20_000, "text", "#7c5cff", { actions: [{ id: "color", label: "Save color" }] }),
];

const SNIPPETS: Snippet[] = [
  { id: 1, name: "Email sign-off", trigger: ";best", body: "Best,\nAlex" },
  { id: 2, name: "Standup template", trigger: ";stand", body: "Yesterday: \nToday: \nBlocked: " },
  { id: 3, name: "Meeting link", trigger: ";meet", body: "https://meet.example/alex" },
  { id: 4, name: "Address", trigger: ";addr", body: "200 Example Street, Oakland, CA" },
];

const COLORS: PickedColor[] = ["#7c5cff", "#ff6b4a", "#16a085", "#f5c542", "#2d3436", "#e84393", "#0984e3", "#b2bec3"].map(
  (hex, i) => ({ id: i + 1, hex, pickedAt: SEC - i * 1_800 }),
);

// ── today ────────────────────────────────────────────────────────────────

const DAY0 = new Date().setHours(0, 0, 0, 0);
const at = (day: 0 | 1, h: number, m = 0) => DAY0 + day * 24 * HOUR + (h * 60 + m) * MIN;
/** Around now, so the agenda has a meeting in progress whatever the hour. */
const near = (offsetMin: number, lengthMin: number) => ({
  start: Math.floor((T0 + offsetMin * MIN) / (15 * MIN)) * 15 * MIN,
  end: Math.floor((T0 + (offsetMin + lengthMin) * MIN) / (15 * MIN)) * 15 * MIN,
});
let evN = 0;
const ev = (title: string, when: { start: number; end: number }, extra: Partial<CalendarEvent> = {}): CalendarEvent => ({
  id: `demo-${evN++}`,
  title,
  ...when,
  allDay: false,
  location: null,
  joinUrl: null,
  calendar: "Work",
  ...extra,
});
const EVENTS: CalendarEvent[] = [
  ev("Ravi's Birthday", { start: at(1, 0), end: at(1, 24) }, { allDay: true, calendar: "Birthdays" }),
  ev("Design sync", near(-10, 40), { joinUrl: "https://meet.example/design" }),
  ev("1:1 with Maya", near(45, 30), { calendar: "Home" }),
  ev("Roadmap review", near(60, 90), { calendar: "Team" }),
  ev("Catch up with Sam", near(240, 120), { calendar: "Home", location: "Nopa" }),
  ev("Morning swim", { start: at(1, 9), end: at(1, 9, 45) }, { calendar: "Home", location: "Home" }),
  ev("Design review", { start: at(1, 10), end: at(1, 13) }, { joinUrl: "https://meet.example/review" }),
  ev("Team lunch", { start: at(1, 13, 30), end: at(1, 14, 30) }, { calendar: "Team" }),
];

const REMINDERS: Reminder[] = [
  { id: "r1", title: "Send standup notes", due: T0 - 2 * HOUR, allDay: false, list: "Work" },
  { id: "r2", title: "Review Rigo's draft", due: T0 + HOUR, allDay: false, list: "Work" },
  { id: "r3", title: "Book club prep", due: DAY0, allDay: true, list: "Reminders" },
  { id: "r4", title: "Pick up arts & crafts supplies", due: T0 + 3 * HOUR, allDay: false, list: "Home" },
  { id: "r5", title: "Cancel membership", due: DAY0 - 24 * HOUR, allDay: true, list: "Reminders" },
  { id: "r6", title: "Schedule car maintenance", due: T0 + 4 * HOUR, allDay: false, list: "Home" },
];

const WEATHER: Weather = {
  place: "Oakland",
  temperature: 21,
  feelsLike: 20,
  sky: "partlyCloudy",
  summary: "Partly cloudy",
  isDay: true,
  days: [
    { day: isoDay(0), sky: "partlyCloudy", high: 24, low: 15 },
    { day: isoDay(-1), sky: "cloudy", high: 20, low: 13 },
    { day: isoDay(-2), sky: "rain", high: 18, low: 12 },
    { day: isoDay(-3), sky: "clear", high: 26, low: 14 },
    { day: isoDay(-4), sky: "clear", high: 28, low: 16 },
  ],
};

const TICKERS: Ticker[] = [
  { symbol: "AAPL", name: "Apple Inc.", price: 232.14, changePct: 0.85, state: "open", delayed: false, error: null, currency: "USD" },
  { symbol: "^GSPC", name: "S&P 500", price: 5712.69, changePct: -1.2, state: "closed", delayed: false, error: null, currency: "USD" },
  { symbol: "BTC-USD", name: "Bitcoin USD", price: 85809.2, changePct: 0.02, state: "open", delayed: true, error: null, currency: "USD" },
  { symbol: "NVDA", name: "NVIDIA Corp.", price: 131.4, changePct: 2.31, state: "open", delayed: false, error: null, currency: "USD" },
];

// ── mac ──────────────────────────────────────────────────────────────────

const MAC_STATS: SystemStats = {
  cpu: 41,
  memoryUsedGb: 11,
  memoryTotalGb: 16,
  diskUsedGb: 212,
  diskTotalGb: 512,
  netDown: 1_400_000,
  netUp: 52_000,
  thermal: "nominal",
};
const BATTERY: Battery = { percent: 82, state: "discharging", minutes: 300, cycles: 120, health: 94 };
const PRIVACY: PrivacyUse = { mic: true, camera: false, micApps: ["Zoom"] };

// ── launch ───────────────────────────────────────────────────────────────

const APP_NAMES = [
  "Safari", "Mail", "Calendar", "Notes", "Messages", "Music", "Spotify", "Slack",
  "Figma", "Xcode", "Terminal", "Arc", "Linear", "Notion", "Photos", "Maps",
];
const APPS: InstalledApp[] = APP_NAMES.map((name) => ({
  name,
  bundle_id: `com.demo.${name.toLowerCase()}`,
  icon: appIcon(name),
}));
const LAUNCHER: LauncherPrefs = {
  pins: ["Safari", "Slack", "Figma", "Terminal"],
  recents: ["Notes", "Spotify", "Linear", "Mail", "Xcode"],
};
const SHORTCUTS = ["Morning routine", "Resize image", "Start focus playlist", "Send ETA", "Log water", "New meeting note"];
const FILES: LaunchFile[] = [
  { path: "/Users/demo/Documents/report-q3.pdf", name: "report-q3.pdf", folder: "Documents" },
  { path: "/Users/demo/Documents/roadmap.md", name: "roadmap.md", folder: "Documents" },
  { path: "/Users/demo/Desktop/mock.png", name: "mock.png", folder: "Desktop" },
  { path: "/Users/demo/Downloads/wireframes.png", name: "wireframes.png", folder: "Downloads" },
  { path: "/Users/demo/Documents/invoice.pdf", name: "invoice.pdf", folder: "Documents" },
];

// ── agents ───────────────────────────────────────────────────────────────

const QUOTA: Quota = {
  fiveHour: { percent: 62, resetsAt: new Date(T0 + 2 * HOUR + 14 * MIN).toISOString() },
  sevenDay: { percent: 38, resetsAt: new Date(T0 + 3 * 86_400_000).toISOString() },
  sevenDayOpus: { percent: 21, resetsAt: new Date(T0 + 3 * 86_400_000).toISOString() },
  extraUsagePercent: 4,
};

const REPOS = [
  { name: "aloud-tauri", path: "/Users/demo/dev/aloud-tauri" },
  { name: "website", path: "/Users/demo/dev/website" },
  { name: "dotfiles", path: "/Users/demo/dev/dotfiles" },
];

// ── the dispatcher ───────────────────────────────────────────────────────

const has = (hay: string, q: unknown) => hay.toLowerCase().includes(String(q ?? "").toLowerCase());
const seen = new Set<string>();

function handle(cmd: string, a: Record<string, unknown> = {}): unknown {
  switch (cmd) {
    case "get_settings": return SETTINGS();
    case "get_stats": return STATS;
    case "islands": {
      // Rust's agenda island, which Home's calendar tile reads instead of Calendar.app.
      const next: Island = {
        id: "calendar:next",
        kind: "live",
        priority: "normal",
        icon: "calendar",
        title: "1:1 with Maya",
        detail: "in 45 min",
        actions: [],
        expires: null,
        dismissible: false,
      };
      const n = Number(params.get("islands"));
      return [next, ...(n > 0 ? livePreviews(n) : params.has("agents") ? livePreviews(1) : [])];
    }
    case "hotkeys_permitted": return true;
    case "meeting_status": return { phase: "idle" };
    case "permissions": return [];
    case "list_permissions": return [];

    case "media_now_playing": return reading();
    case "media_artwork": return cover(268);
    case "media_source_icon": return appIcon(String(a.app ?? "S"));
    case "media_lyrics": return LYRICS;
    case "sound_state": return SOUND;
    case "bluetooth_devices": return DEVICES;

    case "focus_state": return focus();
    case "focus_stats": return FOCUS_STATS;
    case "quick_notes": return NOTES;
    case "note_sync_marks": return Object.entries(NOTE_MARKS).map(([path, state]) => ({ path, state }));
    case "sync_quick_notes": {
      if (params.get("notesSync") === "denied") {
        return { pushed: 0, permission: true, error: "Aloud needs permission to control Notes. Allow it under System Settings, Privacy & Security, Automation, then try again." };
      }
      const todo = Object.keys(NOTE_MARKS).filter((p) => (!a.path || p === a.path) && NOTE_MARKS[p] !== "synced");
      for (const p of todo) NOTE_MARKS[p] = "synced";
      return new Promise((done) => setTimeout(() => done({ pushed: todo.length, permission: false, error: null }), 400));
    }
    case "recent_notes": return NOTES.slice(0, Number(a.limit) || 3);
    case "search_notes":
      return NOTES.filter((n) => has(n.content, a.query)).map((note) => ({ note, score: 1, snippet: note.content.split("\n")[0] }));

    case "shelf_items": return SHELF;
    case "shelf_prefs": return { shots: true, downloads: true, drives: true };
    case "shelf_thumbnail": return art([...String(a.path)].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 11));
    case "shots_recent": return SHOTS.slice(0, Number(a.limit) || SHOTS.length);
    case "downloads_recent": return DOWNLOADS.slice(0, Number(a.limit) || DOWNLOADS.length);
    case "drives_list": return DRIVES;

    case "clipboard_enabled": return true;
    case "clipboard_list": return CLIPS.filter((c) => has(c.preview, a.query));
    case "snippet_list": return SNIPPETS;
    case "color_list": return COLORS;

    case "calendar_events": return EVENTS;
    case "reminders_due": return REMINDERS;
    case "weather":
    case "set_weather_city":
    case "clear_weather_city": return WEATHER;
    case "stock_quotes": return TICKERS;

    case "mac_stats": return MAC_STATS;
    case "mac_battery": return BATTERY;
    case "mac_privacy": return PRIVACY;
    case "mac_front_window": return { trusted: true, app: "Safari" };
    case "mac_watchers": return { privacy: true, battery: true };
    case "mac_snap": return "Moved Safari to the left half";

    case "installed_apps": return APPS;
    case "nudge_state": return { kind: "needsAccess" };
    case "launcher_prefs": return LAUNCHER;
    case "shortcut_list": return SHORTCUTS;
    case "launcher_recent_files": return FILES;
    case "launcher_files": return FILES.filter((f) => has(f.name, a.query));

    case "agent_quota": return QUOTA;
    case "agent_hook_status": return { installed: ["claude", "codex"], conflict: null };
    case "list_local_repos": return REPOS;
    case "list_repo_branches": return ["main", "feat/notch-overhaul", "feat/website"];
    case "history_voice_recent":
    case "history_recent": return VOICE_RUN_PREVIEWS;

    default:
      // Everything that only acts: a press, a toggle, a window the real app moves.
      if (!seen.has(cmd)) {
        seen.add(cmd);
        console.info(`[preview-backend] no fixture for "${cmd}", resolving null`);
      }
      return null;
  }
}

export function installPreviewBackend(): void {
  mockWindows("overlay");
  mockIPC((cmd, args) => handle(cmd, args as Record<string, unknown> | undefined), { shouldMockEvents: true });
}
