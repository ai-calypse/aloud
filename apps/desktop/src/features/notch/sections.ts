import type { ComponentType } from "react";
import type { LucideIcon } from "lucide-react";
import type { NotchStyle } from "@/lib/bindings";
import type { NotchData } from "./context";

/**
 * The notch panel's information architecture (docs/design/notch-sections.html)
 * as types, and the rules that decide what of it is on screen. The data — the
 * nine sections and their views — is `registry.ts`; this file stays free of
 * React views so the rules are testable on their own.
 */

export type SectionId =
  | "now"
  | "agents"
  | "media"
  | "focus"
  | "shelf"
  | "clipboard"
  | "today"
  | "mac"
  | "launch";

export type Subsection = {
  id: string;
  name: string;
  /** Feature names it holds — what ⌘K finds it by besides its own name. */
  features?: string[];
  /**
   * Built and worth showing. A subsection that is not ready does not render
   * at all — there is no "coming soon" — and a section with none ready is
   * hidden. Adding a feature is writing its view and flipping this.
   */
  ready: boolean;
  view?: ComponentType;
  /**
   * The same subsection drawn for the Widgets look. Optional on purpose: a
   * subsection without one shows its calm `view` in either style, so the look
   * can land section by section (see `viewFor`).
   */
  widgetView?: ComponentType;
};

export type Section = {
  id: SectionId;
  name: string;
  icon: LucideIcon;
  /** How many things here need the user right now: the tab's count. */
  count?: (data: NotchData) => number;
  /** One quiet line beside the subsections: what is live here. */
  status?: (data: NotchData) => string;
  subsections: Subsection[];
};

/** Where the panel is: a section, and the subsection last shown in each. */
export type Nav = {
  section: SectionId;
  subs: Partial<Record<SectionId, string>>;
};

/**
 * Only what can be shown: ready subsections that have a view, inside
 * sections left with at least one. Order is the registry's.
 */
export function readySections(sections: Section[]): Section[] {
  return sections
    .map((section) => ({
      ...section,
      subsections: section.subsections.filter((sub) => sub.ready && sub.view),
    }))
    .filter((section) => section.subsections.length > 0);
}

/**
 * What a subsection draws in `style`: its widget view when the user chose
 * Widgets and one exists, else the calm view. The fallback is the point: a
 * subsection nobody has designed as widgets yet is never blank.
 */
export function viewFor(sub: Subsection, style: NotchStyle): ComponentType | undefined {
  return (style === "widgets" && sub.widgetView) || sub.view;
}

/**
 * The body's width. 640 holds up to six labelled section tabs plus ⌘K and
 * close; all nine need the prototype's 780 (measured in Geist at 13px). The
 * widgets look is always 780: four 158pt tiles and their gaps are 698pt wide.
 * Fixed per build and style, so it never changes while a panel is open.
 */
export function panelWidth(readyCount: number, style: NotchStyle): number {
  return style === "widgets" || readyCount > 6 ? 780 : 640;
}

/**
 * The section and subsection `nav` points at, falling back to the first ready
 * one when it names something hidden or gone — a remembered place from a
 * build where that feature existed must not open onto nothing.
 */
export function resolveNav(
  ready: Section[],
  nav: Nav,
): { section: Section; sub: Subsection } | null {
  const section = ready.find((s) => s.id === nav.section) ?? ready[0];
  if (!section) return null;
  const wanted = nav.subs[section.id];
  const sub = section.subsections.find((s) => s.id === wanted) ?? section.subsections[0];
  return { section, sub };
}

/**
 * ⌘1–9 are the registry's positions, not the visible tabs': ⌘2 is Agents in
 * every build, so the shortcut a user learns never moves when a feature lands
 * in front of it. A hidden section's number does nothing.
 */
export function sectionForShortcut(
  all: Section[],
  ready: Section[],
  digit: number,
): Section | null {
  const id = all[digit - 1]?.id;
  return ready.find((s) => s.id === id) ?? null;
}

/** The next or previous ready subsection, wrapping, for Tab and Shift-Tab. */
export function stepSubsection(section: Section, current: string, delta: 1 | -1): string {
  const subs = section.subsections;
  const at = Math.max(0, subs.findIndex((s) => s.id === current));
  return subs[(at + delta + subs.length) % subs.length].id;
}

const NAV_KEY = "aloud.notch.nav";

/**
 * The last place the user chose. Browser storage, not a setting: it is a
 * per-machine convenience that may vanish, and the shell falls back to Now.
 */
export function loadNav(): Nav {
  try {
    const saved = JSON.parse(localStorage.getItem(NAV_KEY) ?? "null") as Nav | null;
    if (saved && typeof saved.section === "string" && typeof saved.subs === "object") {
      return saved;
    }
  } catch {
    /* private window or corrupt value: start from the top */
  }
  return { section: "now", subs: {} };
}

export function saveNav(nav: Nav) {
  try {
    localStorage.setItem(NAV_KEY, JSON.stringify(nav));
  } catch {
    /* storage refused: the place is simply not remembered */
  }
}
