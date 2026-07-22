import { useState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Group } from "@/components/ui/rows";

/**
 * A grouped card of rows the user can add to and delete from, with the
 * enter/exit motion and the lifecycle that makes it correct.
 *
 * Extracted rather than copied into each editor: the part worth sharing is not
 * the markup but the two rules that are easy to get subtly wrong — a removal
 * that must wait for its exit animation to finish, and only one removal in
 * flight at a time, because committing a second mid-exit renumbers the row
 * still animating.
 *
 * Rows are keyed by index, which is safe only because of that second rule: the
 * array does not change while an animation is running.
 */
export function EditableList<T>({
  title,
  items,
  onChange,
  make,
  isBlank,
  problem,
  addLabel,
  empty,
  row,
  children,
}: {
  title: string;
  items: T[];
  onChange: (items: T[]) => void;
  /** A fresh, empty item for the Add button. */
  make: () => T;
  /** Whether an item is still unfilled — Add waits rather than stacking blanks. */
  isBlank: (item: T) => boolean;
  /** Why this item will not do anything, or null if it will. */
  problem?: (item: T, index: number) => string | null;
  addLabel: string;
  empty: { title: string; body: string };
  row: (args: {
    item: T;
    index: number;
    /** True only for a row the user just added. */
    autoFocus: boolean;
    set: (next: T) => void;
    remove: () => void;
  }) => ReactNode;
  /** Explanatory text under the Add button. */
  children?: ReactNode;
}) {
  const [entering, setEntering] = useState<number | null>(null);
  const [leaving, setLeaving] = useState<number | null>(null);

  const blank = items.some(isBlank);

  function add() {
    setEntering(items.length);
    onChange([...items, make()]);
  }

  /**
   * The removal is downstream of the exit finishing — doing both on the click
   * unmounts the row on its first frame and the animation never renders.
   */
  function settle(i: number) {
    if (leaving === i) {
      onChange(items.filter((_, j) => j !== i));
      setLeaving(null);
      setEntering(null);
    } else if (entering === i) {
      setEntering(null);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <Group title={title}>
        {items.length === 0 ? (
          <div className="py-8 text-center text-ui text-ink-2">
            <p className="mb-1 text-[15px] font-semibold text-foreground">
              {empty.title}
            </p>
            <p>{empty.body}</p>
          </div>
        ) : (
          items.map((item, i) => {
            const note = problem?.(item, i) ?? null;
            return (
              <div
                key={i}
                className="motion-row [&:last-child>*]:border-b-0"
                data-motion={
                  leaving === i ? "out" : entering === i ? "in" : undefined
                }
                onAnimationEnd={() => settle(i)}
              >
                <div className="border-b-[0.5px] border-separator">
                  {row({
                    item,
                    index: i,
                    autoFocus: entering === i,
                    set: (next) =>
                      onChange(items.map((x, j) => (j === i ? next : x))),
                    // One at a time; a second commit mid-exit would renumber
                    // the row still animating.
                    remove: () => leaving === null && setLeaving(i),
                  })}
                  {note && (
                    <p className="-mt-1.5 pb-[13px] text-hint text-warning">
                      {note}
                    </p>
                  )}
                </div>
              </div>
            );
          })
        )}
      </Group>

      <div className="flex items-center gap-3">
        <Button
          variant="accent"
          size="row"
          // An unfilled row is already an empty slot; a second one is clutter
          // that persists to disk.
          disabled={blank}
          onClick={add}
        >
          {addLabel}
        </Button>
        {blank && (
          <span className="text-hint text-ink-2">
            Fill in the empty row first.
          </span>
        )}
      </div>

      {children}
    </div>
  );
}

/** The Delete affordance every row in an `EditableList` carries. */
export function DeleteRow({ onClick }: { onClick: () => void }) {
  return (
    <Button variant="plain-danger" size="text" onClick={onClick}>
      Delete
    </Button>
  );
}
