import { useEffect, useRef } from "react";
import { SearchIcon, XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Materialize } from "@/components/ui/motion";
import { Kbd } from "@/components/notch/controls";
import { SectionTabs } from "@/components/notch/section-tabs";
import { SubTabs } from "@/components/notch/segmented";
import { runAction } from "./actions";
import { useNotch } from "./context";
import { shellIntent } from "./keys";
import { Palette } from "./Palette";
import type { PaletteEntry } from "./search";
import { READY, SECTIONS } from "./registry";
import { resolveNav, sectionForShortcut, stepSubsection, type Nav } from "./sections";

const PANEL_ID = "notch-panel";

/**
 * The expanded notch panel: section tabs, the open section's subsections with
 * its status line, the subsection's view, and a footer naming the keys.
 *
 * Fixed width, height to fit: the tab strip and its underline never move
 * sideways, while a one-line voice reply stays a small panel and a long list
 * stops at the body's ceiling and scrolls. Every height change rides the
 * panel's own live spring (useNotchPanel measures this whole node), so a tab
 * switched mid-morph bends the morph rather than restarting it.
 */
export function NotchShell({ nav }: { nav: Nav }) {
  const { navigate, close, palette, setPalette, ...data } = useNotch();
  const body = useRef<HTMLDivElement>(null);
  const place = resolveNav(READY, nav);
  // Read by the key handler, which subscribes once rather than per render.
  const placeRef = useRef(place);
  placeRef.current = place;

  // The keyboard. On the window, not the panel: the overlay window holds key
  // focus while a panel is open, but nothing inside it may be focused yet.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const place = placeRef.current;
      if (!place) return;
      const target = e.target instanceof Element ? e.target : null;
      const intent = shellIntent({
        key: e.key,
        metaKey: e.metaKey,
        ctrlKey: e.ctrlKey,
        shiftKey: e.shiftKey,
        altKey: e.altKey,
        editing: !!target?.closest("input, textarea, select, [contenteditable='true']"),
        inBody: !!target && !!body.current?.contains(target),
      });
      if (!intent) return;
      e.preventDefault();
      if (intent.kind === "palette") {
        setPalette(true);
      } else if (intent.kind === "section") {
        const section = sectionForShortcut(SECTIONS, READY, intent.digit);
        if (section) {
          setPalette(false);
          navigate(section.id);
        }
      } else {
        navigate(place.section.id, stepSubsection(place.section, place.sub.id, intent.delta));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate, setPalette]);

  if (!place) return null;
  const { section, sub } = place;
  const View = sub.view!;
  const status = section.status?.(data) ?? "";
  // One index across every subsection, so Materialize enters from the side
  // the user moved towards whether they changed section or subsection.
  const flat = READY.flatMap((s) => s.subsections.map((x) => `${s.id}/${x.id}`)).indexOf(
    `${section.id}/${sub.id}`,
  );

  const pick = (entry: PaletteEntry) => {
    setPalette(false);
    if (entry.to) navigate(entry.to.section, entry.to.sub);
    if (entry.action) void runAction(entry.action, { open: navigate });
  };

  return (
    <div className="notch-shell flex w-[640px] flex-col font-notch text-notch-ink">
      <header className="flex items-center gap-1 border-b border-notch-sep px-3.5">
        <SectionTabs
          className="flex-1"
          panelId={PANEL_ID}
          value={section.id}
          onChange={(id) => {
            setPalette(false);
            navigate(id as Nav["section"]);
          }}
          onExit={() => focusFirst(body.current)}
          items={READY.map((s) => ({
            id: s.id,
            label: s.name,
            icon: s.icon,
            count: s.count?.(data),
            shortcut: `⌘${SECTIONS.findIndex((x) => x.id === s.id) + 1}`,
          }))}
        />
        <Button
          variant="notch-quiet"
          size="notch"
          aria-label="Search sections, features and actions"
          aria-pressed={palette}
          className={palette ? "bg-notch-well text-notch-ink" : undefined}
          onClick={() => setPalette(!palette)}
        >
          <SearchIcon />
          <Kbd>⌘K</Kbd>
        </Button>
        <Button variant="notch-quiet" size="notch-icon" aria-label="Close" title="Close (esc)" onClick={close}>
          <XIcon />
        </Button>
      </header>

      <div className="flex min-h-[44px] items-center justify-between gap-3 px-3.5 pt-2.5">
        {/* A lone subsection needs no switcher; its name is the section's. */}
        {section.subsections.length > 1 ? (
          <SubTabs
            panelId={PANEL_ID}
            value={sub.id}
            onChange={(id) => {
              setPalette(false);
              navigate(section.id, id);
            }}
            items={section.subsections.map((s) => ({ id: s.id, label: s.name }))}
          />
        ) : (
          <span className="text-notch-body font-medium">{sub.name}</span>
        )}
        <span className="truncate text-notch-meta text-notch-ink-3" aria-live="polite">
          {status}
        </span>
      </div>

      <div
        ref={body}
        id={PANEL_ID}
        role="tabpanel"
        aria-labelledby={`notch-tab-${section.id}`}
        className="max-h-[420px] overflow-y-auto px-4 pt-3.5 pb-3"
      >
        {palette && <Palette onPick={pick} onClose={() => setPalette(false)} />}
        {/* Hidden, not unmounted, under the palette: a run's approval gate
            keeps its draft and its countdown while the user looks something
            up. */}
        <div hidden={palette}>
          <Materialize index={flat}>
            <View />
          </Materialize>
        </div>
      </div>

      <footer className="flex items-center justify-end gap-3.5 border-t border-notch-sep px-4 py-2 text-notch-label text-notch-ink-3">
        <span className="flex items-center gap-1.5">
          <Kbd>⌘1–9</Kbd> sections
        </span>
        <span className="flex items-center gap-1.5">
          <Kbd>⇥</Kbd> next
        </span>
        <span className="flex items-center gap-1.5">
          <Kbd>⌘K</Kbd> search
        </span>
        <span className="flex items-center gap-1.5">
          <Kbd>esc</Kbd> close
        </span>
      </footer>
    </div>
  );
}

/** Hands keyboard focus to the first control in the body, for ↓ from the tabs. */
function focusFirst(root: HTMLElement | null) {
  root
    ?.querySelector<HTMLElement>(
      "button:not(:disabled), [href], input:not(:disabled), textarea, select, [tabindex]:not([tabindex='-1'])",
    )
    ?.focus();
}
