import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";
import { createSpring, type Spring } from "@/lib/spring";

/** Critically damped, like every other travel here; only the response varies. */
const FEEL = { damping: 1, response: 0.31 };

type Item = {
  el: HTMLElement;
  /** Where layout puts it now, and where it visibly is. */
  nat: { x: number; y: number };
  cur: { x: number; y: number };
  x: Spring;
  y: Spring;
  placed: boolean;
};

/**
 * FLIP for a grid whose items change cell: each `[data-tile]` springs from
 * where it was seen to where layout now puts it. The target can change while
 * the item is still travelling (a timer starting, then a track), and the whole
 * point of `createSpring` is that a retarget keeps the momentum, which a CSS
 * transition restarting from rest cannot.
 *
 * The spring runs in the item's *absolute* position and the transform is its
 * difference from layout, so a relayout only moves the target and the
 * velocity carries through. Positions are `offsetLeft/Top` (layout, not
 * transformed) against `root`, which must be positioned.
 *
 * `layout` is anything that changes when cells do. Size changes are not this
 * hook's: the tile snaps to its new size and its content crossfades.
 * Reduced motion needs no code here: `createSpring` jumps instead of
 * travelling.
 */
export function useFlip(root: RefObject<HTMLElement | null>, layout: unknown) {
  const items = useRef(new Map<string, Item>());

  useLayoutEffect(() => {
    for (const el of root.current?.querySelectorAll<HTMLElement>("[data-tile]") ?? []) {
      const key = el.dataset.tile!;
      let item = items.current.get(key);
      if (item?.el !== el) {
        item?.x.stop();
        item?.y.stop();
        item = create(el);
        items.current.set(key, item);
      }
      item.nat = { x: el.offsetLeft, y: el.offsetTop };
      if (!item.placed) {
        item.placed = true;
        item.cur = { ...item.nat };
        item.x.jump(item.nat.x);
        item.y.jump(item.nat.y);
        continue;
      }
      // Show the old visual spot at once, then let the springs close the gap.
      paint(item);
      item.x.to(item.nat.x);
      item.y.to(item.nat.y);
    }
  }, [root, layout]);

  useEffect(() => {
    const all = items.current;
    return () => {
      for (const item of all.values()) {
        item.x.stop();
        item.y.stop();
      }
      all.clear();
    };
  }, []);
}

function create(el: HTMLElement): Item {
  const item: Item = {
    el,
    nat: { x: 0, y: 0 },
    cur: { x: 0, y: 0 },
    placed: false,
    x: createSpring((v) => {
      item.cur.x = v;
      paint(item);
    }, FEEL),
    y: createSpring((v) => {
      item.cur.y = v;
      paint(item);
    }, FEEL),
  };
  return item;
}

function paint(item: Item) {
  const dx = item.cur.x - item.nat.x;
  const dy = item.cur.y - item.nat.y;
  item.el.style.transform = dx || dy ? `translate(${dx}px, ${dy}px)` : "";
}
