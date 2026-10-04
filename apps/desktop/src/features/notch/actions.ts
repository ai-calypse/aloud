import { commands, type Player, type Transport } from "@/lib/bindings";
import type { SectionId } from "./sections";

/** What an action may do to the notch itself. */
export type ActionContext = {
  /** Opens the panel, if it is closed, at a section and subsection. */
  open: (section: SectionId, sub?: string) => void;
};

/**
 * Something the notch can do, by name. One registry for every way in: the
 * ⌘K palette lists these, an island's buttons name them (Rust sends the id,
 * never code), and the voice agent can call them by id next.
 */
export type NotchAction = {
  id: string;
  title: string;
  /** Other words people would search for it by. */
  keywords?: string[];
  /** Where it lives, for the palette's hint and for grouping. */
  section?: SectionId;
  subsection?: string;
  /**
   * Arguments it cannot run without. An action that needs any is only for a
   * caller that has them — an island, the agent — and ⌘K does not offer it.
   */
  args?: string[];
  /**
   * Does it. Resolves true when it did what it says, which is when an island
   * that offered it may go; false keeps the island up to say so.
   */
  run: (ctx: ActionContext, args: Record<string, string>) => boolean | Promise<boolean>;
};

/**
 * The player a media action means: the one an island names (Rust sends the
 * app's name), else whichever is on — ⌘K has no island to ask.
 */
async function mediaPlayer(args: Record<string, string>): Promise<Player | null> {
  if (args.player === "Spotify") return "spotify";
  if (args.player === "Music") return "music";
  return (await commands.mediaNowPlaying()).now?.player ?? null;
}

/** Sends a transport command to the player. False when nothing is playing or the player refuses. */
async function transport(args: Record<string, string>, command: Transport): Promise<boolean> {
  const player = await mediaPlayer(args);
  return player !== null && (await commands.mediaControl(player, command)).status === "ok";
}

export const ACTIONS: NotchAction[] = [
  {
    id: "agents.open",
    title: "Open coding agents",
    keywords: ["claude", "codex", "approvals", "permission"],
    section: "agents",
    subsection: "coding",
    run: (ctx) => {
      ctx.open("agents", "coding");
      return true;
    },
  },
  {
    id: "agents.voice",
    title: "Show the last voice command",
    keywords: ["command", "run", "steps"],
    section: "agents",
    subsection: "voice",
    run: (ctx) => {
      ctx.open("agents", "voice");
      return true;
    },
  },
  {
    id: "agents.usage",
    title: "Show plan usage",
    keywords: ["quota", "limits", "claude"],
    section: "agents",
    subsection: "usage",
    run: (ctx) => {
      ctx.open("agents", "usage");
      return true;
    },
  },
  // notch-media: the Media section's actions.
  {
    id: "media.toggle",
    title: "Play or pause",
    keywords: ["music", "spotify", "song", "resume", "stop"],
    section: "media",
    subsection: "playing",
    run: (_, args) => transport(args, "toggle"),
  },
  {
    id: "media.next",
    title: "Next track",
    keywords: ["skip", "music", "spotify", "song"],
    section: "media",
    subsection: "playing",
    run: (_, args) => transport(args, "next"),
  },
  {
    id: "media.previous",
    title: "Previous track",
    keywords: ["back", "music", "spotify", "song"],
    section: "media",
    subsection: "playing",
    run: (_, args) => transport(args, "previous"),
  },
  {
    id: "media.open",
    title: "Show what's playing",
    keywords: ["music", "spotify", "now playing", "track"],
    section: "media",
    subsection: "playing",
    run: (ctx) => {
      ctx.open("media", "playing");
      return true;
    },
  },
  {
    id: "media.lyrics",
    title: "Show lyrics",
    keywords: ["song", "words", "sing", "karaoke"],
    section: "media",
    subsection: "lyrics",
    run: (ctx) => {
      ctx.open("media", "lyrics");
      return true;
    },
  },
  {
    id: "sound.micMute",
    title: "Mute or unmute the microphone",
    keywords: ["mic", "meeting", "zoom", "silence", "audio"],
    section: "media",
    subsection: "sound",
    run: async () => {
      const { micMuted } = await commands.soundState();
      // No switch on this microphone: say so rather than pretend.
      return micMuted !== null && (await commands.soundSetMicMuted(!micMuted)).status === "ok";
    },
  },
  {
    id: "sound.open",
    title: "Choose an audio output or input",
    keywords: ["speakers", "headphones", "airpods", "switch", "audio device"],
    section: "media",
    subsection: "sound",
    run: (ctx) => {
      ctx.open("media", "sound");
      return true;
    },
  },
  {
    id: "devices.open",
    title: "Show AirPods and device batteries",
    keywords: ["bluetooth", "headphones", "mouse", "keyboard", "battery", "airpods"],
    section: "media",
    subsection: "devices",
    run: (ctx) => {
      ctx.open("media", "devices");
      return true;
    },
  },
  // -- Clipboard section --
  {
    id: "clipboard.history",
    title: "Open clipboard history",
    keywords: ["copy", "paste", "copied", "ocr"],
    section: "clipboard",
    subsection: "history",
    run: (ctx) => {
      ctx.open("clipboard", "history");
      return true;
    },
  },
  {
    id: "clipboard.snippets",
    title: "Open snippets",
    keywords: ["text", "template", "boilerplate"],
    section: "clipboard",
    subsection: "snippets",
    run: (ctx) => {
      ctx.open("clipboard", "snippets");
      return true;
    },
  },
  {
    id: "colors.pick",
    title: "Pick a color from the screen",
    keywords: ["eyedropper", "hex", "rgb", "sample"],
    section: "clipboard",
    subsection: "colors",
    run: async () => (await commands.colorPick()).status === "ok",
  },
  // -- Launch section --
  {
    id: "launch.apps",
    title: "Open the app launcher",
    keywords: ["open", "start", "applications"],
    section: "launch",
    subsection: "apps",
    run: (ctx) => {
      ctx.open("launch", "apps");
      return true;
    },
  },
  {
    id: "launch.shortcuts",
    title: "Run an Apple Shortcut",
    keywords: ["automation", "workflow"],
    section: "launch",
    subsection: "shortcuts",
    run: (ctx) => {
      ctx.open("launch", "shortcuts");
      return true;
    },
  },
  {
    id: "launch.search",
    title: "Search apps, notes and files",
    keywords: ["find", "spotlight", "open"],
    section: "launch",
    subsection: "search",
    run: (ctx) => {
      ctx.open("launch", "search");
      return true;
    },
  },
  {
    // What a finished row's Show does: bring back what a write made.
    id: "result.show",
    title: "Show what was made",
    args: ["app", "target"],
    run: (_, { app, target }) => {
      void commands.showResult(app, target);
      return true;
    },
  },
  {
    // A tidy is the one write with a reverse already built.
    id: "files.undoTidy",
    title: "Undo a tidy",
    args: ["folder"],
    run: async (_, { folder }) => (await commands.undoTidy(folder)).status === "ok",
  },

  // -- notch Shelf --
  {
    id: "shelf.files",
    title: "Open the file shelf",
    keywords: ["shelf", "drop", "airdrop", "zip", "convert", "image", "files"],
    section: "shelf",
    subsection: "files",
    run: (ctx) => {
      ctx.open("shelf", "files");
      return true;
    },
  },
  {
    id: "shelf.shots",
    title: "Show recent screenshots",
    keywords: ["screenshot", "screen capture", "pictures"],
    section: "shelf",
    subsection: "shots",
    run: (ctx) => {
      ctx.open("shelf", "shots");
      return true;
    },
  },
  {
    id: "shelf.downloads",
    title: "Show recent downloads",
    keywords: ["downloaded", "downloads folder"],
    section: "shelf",
    subsection: "downloads",
    run: (ctx) => {
      ctx.open("shelf", "downloads");
      return true;
    },
  },
  {
    id: "shelf.drives",
    title: "Show external drives",
    keywords: ["eject", "usb", "disk", "volume", "sd card"],
    section: "shelf",
    subsection: "drives",
    run: (ctx) => {
      ctx.open("shelf", "drives");
      return true;
    },
  },
  {
    id: "shelf.capture",
    title: "Capture a screenshot region",
    keywords: ["screenshot", "screencapture", "snip", "crop"],
    section: "shelf",
    subsection: "shots",
    run: async () => (await commands.shotsCapture("region")).status === "ok",
  },
  {
    id: "shelf.captureClipboard",
    title: "Capture a region to the clipboard",
    keywords: ["screenshot", "screencapture", "copy"],
    section: "shelf",
    subsection: "shots",
    run: async () => (await commands.shotsCapture("clipboard")).status === "ok",
  },
  {
    id: "shelf.copyShot",
    title: "Copy a screenshot",
    args: ["path"],
    run: async (_, { path }) => (await commands.shotsCopy(path)).status === "ok",
  },
  {
    id: "shelf.reveal",
    title: "Show a file in Finder",
    args: ["path"],
    run: async (_, { path }) => (await commands.finderReveal(path)).status === "ok",
  },
  {
    id: "shelf.eject",
    title: "Eject a drive",
    args: ["mount"],
    run: async (_, { mount }) => (await commands.driveEject(mount)).status === "ok",
  },
  // -- end notch Shelf --
];

/**
 * Runs an action by id. False for an id nobody registered, for missing
 * arguments, and for a run that throws — the browser preview has no Tauri,
 * and an island button must fail visibly rather than take the overlay down.
 */
export async function runAction(
  id: string,
  ctx: ActionContext,
  args: Record<string, string> = {},
  registry: NotchAction[] = ACTIONS,
): Promise<boolean> {
  const action = registry.find((a) => a.id === id);
  if (!action) return false;
  if (action.args?.some((name) => !(name in args))) return false;
  try {
    return await action.run(ctx, args);
  } catch {
    return false;
  }
}
