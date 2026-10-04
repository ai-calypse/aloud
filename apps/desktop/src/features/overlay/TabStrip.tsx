import { PlusIcon, XIcon } from "lucide-react";

/**
 * One tab in the voice run's strip. Tab 0 is the live agent run; the rest are
 * coding-agent composers. `closing` is set instead of removing the tab, so its
 * exit has something to play on — see `closeTab`.
 */
export type Tab = { id: number; blank: boolean; closing?: boolean };

/** The strip a run starts with: just the panel the dictation opened. */
export const FIRST_TABS: Tab[] = [{ id: 0, blank: false }];

/**
 * Where `+` stops. The strip has the panel's width and no more, and past
 * this every label is ellipsed to two characters — a notch panel is not a
 * browser. ponytail: a hard cap, not overflow scrolling; if tabs ever carry
 * enough to want more, the strip needs its own overflow affordance anyway.
 */
export const MAX_TABS = 5;

/**
 * The voice run's tab strip, inside Agents › Voice. Terminal-style: the run
 * that opened the panel is the first tab, `+` puts a blank one (a coding-agent
 * composer) beside it. The coding agents themselves are their own subsection
 * now, so the strip no longer carries an Agents chip.
 *
 * Sizes nothing — it lives inside the fixed-width panel, so adding a tab moves
 * the panel's shoulders not at all and only the swapped body retargets the
 * height spring.
 */
export function TabStrip({
  tabs,
  active,
  /** Whether an agent run exists — its tab only shows then. */
  runOpen,
  /** What tab 0 is called: the question asked, or what the agent heard. */
  title,
  onSelect,
  onClose,
  onClosed,
  onAdd,
}: {
  tabs: Tab[];
  active: number;
  runOpen: boolean;
  title: string;
  onSelect: (id: number) => void;
  onClose: (id: number) => void;
  onClosed: (id: number) => void;
  onAdd: () => void;
}) {
  return (
    <div className="tabs">
      {/* The run's own tab only exists while a run does; blank tabs belong
          to the strip and open from anywhere — the notch is one surface. */}
      {tabs
        .filter((tab) => runOpen || tab.blank)
        .map((tab, i) => (
        // The wrapper is what collapses; the chip inside it is what is clipped.
        <div
          key={tab.id}
          className="tab"
          data-motion={tab.closing ? "out" : "in"}
          // Guarded on the target: animationend bubbles, and an animation added
          // inside a chip later would otherwise delete the tab under it.
          onAnimationEnd={(e) =>
            tab.closing && e.target === e.currentTarget && onClosed(tab.id)
          }
        >
          <div className="tab__chip" data-active={tab.id === active || undefined}>
            <button className="tab__select" onClick={() => onSelect(tab.id)}>
              {/* A blank tab is a coding-agent composer, so name it one —
                  numbered by position, not id, so the strip reads 1..n after a
                  close and two of them stay switchable-between. */}
              {tab.blank ? `Agent ${i + (runOpen ? 0 : 1)}` : title}
            </button>
            {/* Tab 0 is the run itself; closing it would leave the panel with
                nothing to show but no way back. Escape still closes the lot. */}
            {tab.id !== 0 && (
              <button
                className="tab__close"
                aria-label="Close tab"
                onClick={() => onClose(tab.id)}
              >
                <XIcon size={11} />
              </button>
            )}
          </div>
        </div>
      ))}
      {tabs.length < MAX_TABS && (
        <button className="tab__add" aria-label="New tab" title="New tab" onClick={onAdd}>
          {/* Heavier than the default 2 at this size — a hairline plus on black
              disappears into the panel. */}
          <PlusIcon size={15} strokeWidth={2.5} />
        </button>
      )}
    </div>
  );
}
