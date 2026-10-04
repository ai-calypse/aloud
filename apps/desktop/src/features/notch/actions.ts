import { commands } from "@/lib/bindings";
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
  // notch-today-focus: Focus actions begin
  {
    id: "focus.start",
    title: "Start a focus timer",
    keywords: ["pomodoro", "timer", "focus", "work"],
    section: "focus",
    subsection: "timer",
    run: async (ctx) => {
      ctx.open("focus", "timer");
      await commands.focusStart("pomodoro", { focus: 25, rest: 5 });
      return true;
    },
  },
  {
    id: "focus.stopwatch",
    title: "Start the stopwatch",
    keywords: ["timer", "count up", "time"],
    section: "focus",
    subsection: "timer",
    run: async (ctx) => {
      ctx.open("focus", "timer");
      await commands.focusStart("stopwatch", { focus: 25, rest: 5 });
      return true;
    },
  },
  // Island buttons: Pause on a running timer, Resume on a paused one or a
  // phase that is waiting to start.
  {
    id: "focus.pause",
    title: "Pause the timer",
    section: "focus",
    subsection: "timer",
    run: async () => {
      await commands.focusPause();
      return true;
    },
  },
  {
    id: "focus.resume",
    title: "Resume the timer",
    keywords: ["start break", "continue"],
    section: "focus",
    subsection: "timer",
    run: async () => {
      await commands.focusResume();
      return true;
    },
  },
  {
    id: "focus.stop",
    title: "Stop the timer",
    keywords: ["end", "reset", "cancel"],
    section: "focus",
    subsection: "timer",
    run: async () => {
      await commands.focusStop();
      return true;
    },
  },
  {
    id: "focus.awake",
    title: "Keep my Mac awake for 2 hours",
    keywords: ["caffeinate", "sleep", "awake", "display"],
    section: "focus",
    subsection: "awake",
    run: async (ctx) => {
      ctx.open("focus", "awake");
      return (await commands.awakeStart(120)).status === "ok";
    },
  },
  // Island button on the keep-awake island.
  {
    id: "focus.awakeOff",
    title: "Let my Mac sleep again",
    keywords: ["caffeinate", "stop", "awake"],
    section: "focus",
    subsection: "awake",
    run: async () => {
      await commands.awakeStop();
      return true;
    },
  },
  {
    id: "focus.note",
    title: "Write a quick note",
    keywords: ["note", "jot", "write"],
    section: "focus",
    subsection: "notes",
    run: (ctx) => {
      ctx.open("focus", "notes");
      return true;
    },
  },
  // notch-today-focus: Focus actions end
  // notch-today-focus: Today actions begin
  {
    id: "today.calendar",
    title: "Show today's calendar",
    keywords: ["events", "meetings", "schedule", "agenda"],
    section: "today",
    subsection: "calendar",
    run: (ctx) => {
      ctx.open("today", "calendar");
      return true;
    },
  },
  {
    id: "today.reminders",
    title: "Show today's reminders",
    keywords: ["todo", "tasks", "due"],
    section: "today",
    subsection: "reminders",
    run: (ctx) => {
      ctx.open("today", "reminders");
      return true;
    },
  },
  {
    id: "today.weather",
    title: "Show the weather",
    keywords: ["forecast", "temperature", "rain"],
    section: "today",
    subsection: "weather",
    run: (ctx) => {
      ctx.open("today", "weather");
      return true;
    },
  },
  {
    id: "today.stocks",
    title: "Show my stocks",
    keywords: ["ticker", "prices", "market"],
    section: "today",
    subsection: "stocks",
    run: (ctx) => {
      ctx.open("today", "stocks");
      return true;
    },
  },
  // The next-event island's Join button.
  {
    id: "calendar.join",
    title: "Join the call",
    args: ["url"],
    run: async (_, { url }) => (await commands.openMeetingLink(url)).status === "ok",
  },
  // notch-today-focus: Today actions end
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
