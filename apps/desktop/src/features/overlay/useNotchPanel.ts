import { useEffect, useRef, useState } from "react";
import type { OverlayStyle } from "@/lib/bindings";
import { notchPath } from "@/lib/notch-shape";
import { createSpring } from "@/lib/spring";

/**
 * Only for a stylesheet that has not applied yet; motion.css owns the springs
 * and tokens.css the fully open corner radius.
 */
const PANEL_FALLBACK = { damping: 0.77, open: 0.45, close: 0.55, radius: 24 };

function readPanelSpring(element: Element) {
  const computed = getComputedStyle(element);
  const read = (name: string, fallback: number) => {
    const value = parseFloat(computed.getPropertyValue(name));
    return Number.isFinite(value) ? value : fallback;
  };
  return {
    damping: read("--panel-damping", PANEL_FALLBACK.damping),
    open: read("--panel-open", PANEL_FALLBACK.open),
    close: read("--panel-close", PANEL_FALLBACK.close),
    radius: read("--radius-notch-panel", PANEL_FALLBACK.radius),
  };
}

/**
 * How the corner radii open up as the panel grows past the cutout. Derived from
 * the *current* height, not the target, so they interpolate with the box rather
 * than jumping to their final value on the first frame.
 */
const RADIUS = { topGrowth: 0.12, topMax: 16, bottomGrowth: 0.14 };

/** Only for the `?style=pill` browser preview, where no notch is measured. */
const NOTCH_FALLBACK = { width: 200, height: 32 };

/**
 * Minimum panel width, as a multiple of the cutout, so it is never narrower
 * than the hole it hangs from. Wider content still wins.
 */
const PANEL_WIDTH_FACTOR = 1.5;

/**
 * Grows the drop-down panel out of the notch and shrinks it back in.
 *
 * The one surface that is not a CSS transition: its size is retargeted
 * mid-flight three times a dictation, and a baked curve redirected halfway
 * restarts from zero velocity — hence `lib/spring.ts`. Writes the box and the
 * shape's `d` straight onto the nodes, so no React render sits between frames.
 */
export function useNotchPanel(
  style: OverlayStyle,
  run: number,
  /**
   * Whether the panel currently has content worth measuring. Part of the
   * rebuild key: the agents surface and the hover peek appear without a
   * dictation ever bumping `run`, and a driver built against an empty (or
   * `display: none`) body measured nothing — so the box never reached Rust
   * and every click on a visible permission card fell through to the app
   * behind the notch.
   */
  populated: boolean,
  /**
   * The resting strip is showing beside the cutout and owns the resting
   * click-box itself (it is wider than the cutout, and off-centre-left) so that
   * hovering it flips Rust's `hovering` and grows the notch. While that is true
   * the panel must not also write the box in its *unpopulated* branch, or the
   * two writers fight through the same report. Once hovering grows the notch
   * (populated), the panel reclaims the box — the strip bows out (`handedOff`).
   */
  ambientOwnsBox: boolean,
  dismissing: boolean,
  /** Called with the box the panel is settling to, whenever that changes. */
  onTarget: (width: number, height: number) => void,
) {
  const surface = useRef<HTMLDivElement>(null);
  const shape = useRef<SVGSVGElement>(null);
  const path = useRef<SVGPathElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const driver = useRef<{
    open: () => void;
    collapse: () => void;
    remeasure: () => void;
  } | null>(null);
  // Bumped when the body mounts populated but has not laid out yet (offsetWidth
  // 0), so the build effect re-runs the instant it gets a size. Without it a
  // first paint with zero-width content bailed and never re-measured — the
  // notch grew on hover but reported only the cutout box, so Rust's poll judged
  // the cursor (now over a card) as outside and the hover never took until a
  // click forced a fresh cycle.
  const [layoutTick, setLayoutTick] = useState(0);

  // Rebuilt each time the panel appears, so every dictation starts its growth
  // from the cutout rather than from wherever the last one left the springs.
  useEffect(() => {
    const surfaceElement = surface.current;
    const shapeElement = shape.current;
    const pathElement = path.current;
    const contentElement = content.current;
    if (!surfaceElement || !shapeElement || !pathElement || !contentElement) return;
    // Nothing to show: at rest the surface *is* the cutout, nothing more.
    // Snap it there rather than leaving whatever inline size the last
    // driver's springs wrote — an empty panel keeping its old size is a
    // black slab hanging under the menu bar. The cutout box is still
    // reported, so hovering that strip is what grows the notch.
    //
    // Judged from `populated`, never from the body's measured width: a
    // `display: none` pill measures 0 while holding real content, and an
    // empty body measures its own padding — both lied here before.
    if (!populated) {
      const computed = getComputedStyle(surfaceElement);
      const measured = (name: string, fallback: number) => {
        const value = parseFloat(computed.getPropertyValue(name));
        return Number.isFinite(value) ? value : fallback;
      };
      const width = measured("--notch-w", NOTCH_FALLBACK.width);
      const height = measured("--notch-h", NOTCH_FALLBACK.height);
      surfaceElement.style.width = `${width}px`;
      surfaceElement.style.height = `${height}px`;
      shapeElement.setAttribute("viewBox", `0 0 ${width} ${height}`);
      pathElement.setAttribute(
        "d",
        notchPath(width, height, Math.max(0, height / 3 - 4), height / 3),
      );
      // When the ambient strip is up it owns the click-box (it is wider than
      // the cutout and reaches left of it); reporting the bare cutout here
      // would clobber that and the strip would stop taking clicks.
      if (!ambientOwnsBox) onTarget(width, height);
      return;
    }
    // On screen but not laid out yet (just mounted, or `display: none`):
    // nothing to measure. Watch for the layout and bump `layoutTick` to re-run
    // this effect the instant it has a size — `populated`/`run` may not change
    // again (a first hover flips `populated` once), so relying on them to come
    // back around stranded the notch reporting only the cutout box.
    if (!contentElement.offsetWidth) {
      const pending = new ResizeObserver(() => {
        if (contentElement.offsetWidth) setLayoutTick((n) => n + 1);
      });
      pending.observe(contentElement);
      return () => pending.disconnect();
    }

    // Read once per rebuild, not per retarget: it is a style recalc, and
    // `remeasure` fires once a frame while an answer streams.
    const PANEL = readPanelSpring(surfaceElement);

    let base = { width: NOTCH_FALLBACK.width, height: NOTCH_FALLBACK.height };
    let target = base;
    let baseRadius = NOTCH_FALLBACK.height / 3;
    let collapsed = false;

    // getComputedStyle forces a style recalc, so it is called when the layout
    // changes and never from inside a frame.
    const measure = () => {
      const computed = getComputedStyle(surfaceElement);
      // A screen with no notch reports a real zero; `||` would conflate that
      // with the variable being absent and invent a cutout on an iMac. Only a
      // non-number falls back.
      const measured = (name: string, fallback: number) => {
        const value = parseFloat(computed.getPropertyValue(name));
        return Number.isFinite(value) ? value : fallback;
      };
      const notchWidth = measured("--notch-w", NOTCH_FALLBACK.width);
      const notchHeight = measured("--notch-h", NOTCH_FALLBACK.height);

      // The cutout's own rounding; every other radius is measured from it.
      baseRadius = notchHeight / 3;
      base = { width: notchWidth, height: notchHeight };
      const settledHeight = notchHeight + contentElement.offsetHeight;
      target = {
        // The body sits between the two concave shoulders, so the panel is
        // wider than its content by exactly that much — measured at the
        // *settled* top radius. Never narrower than the cutout, or the physical
        // camera bump sticks out either side.
        width: Math.max(
          notchWidth * PANEL_WIDTH_FACTOR,
          contentElement.offsetWidth + topRadiusAt(settledHeight) * 2,
        ),
        height: settledHeight,
      };
      onTarget(target.width, target.height);
    };

    const topRadiusAt = (height: number) =>
      Math.min(
        RADIUS.topMax,
        Math.max(0, baseRadius - 4) + Math.max(0, height - base.height) * RADIUS.topGrowth,
      );

    const size = { width: 0, height: 0 };
    const draw = () => {
      // Floored at the cutout: the spring rings a fifth of its travel past the
      // target, which on the way back in undershoots clean through the 32pt
      // cutout and springs back out. Only what is drawn is clamped — the spring
      // keeps its real state, so the motion's timing is unchanged.
      const width = Math.max(base.width, size.width);
      const height = Math.max(base.height, size.height);

      const grown = height - base.height;
      const bottomRadius = Math.min(
        PANEL.radius,
        baseRadius + grown * RADIUS.bottomGrowth,
      );

      surfaceElement.style.width = `${width}px`;
      surfaceElement.style.height = `${height}px`;
      shapeElement.setAttribute("viewBox", `0 0 ${width} ${height}`);
      pathElement.setAttribute(
        "d",
        notchPath(width, height, topRadiusAt(height), bottomRadius),
      );
    };

    const width = createSpring(
      (value) => {
        size.width = value;
        draw();
      },
      { damping: PANEL.damping, response: PANEL.open },
    );
    const height = createSpring(
      (value) => {
        size.height = value;
        draw();
      },
      { damping: PANEL.damping, response: PANEL.open },
    );

    measure();
    size.width = base.width;
    size.height = base.height;
    width.jump(base.width);
    height.jump(base.height);
    width.to(target.width, PANEL.open);
    height.to(target.height, PANEL.open);

    driver.current = {
      open() {
        collapsed = false;
        measure();
        width.to(target.width, PANEL.open);
        height.to(target.height, PANEL.open);
      },
      collapse() {
        collapsed = true;
        width.to(base.width, PANEL.close);
        height.to(base.height, PANEL.close);
      },
      remeasure() {
        measure();
        // A label that changes while the panel is collapsing must not drag it
        // back open — the window is already on its way out.
        if (collapsed) return;
        width.to(target.width, PANEL.open);
        height.to(target.height, PANEL.open);
      },
    };

    const observer = new ResizeObserver(() => driver.current?.remeasure());
    observer.observe(contentElement);

    return () => {
      observer.disconnect();
      width.stop();
      height.stop();
      driver.current = null;
    };
  }, [style, run, populated, ambientOwnsBox, layoutTick]);

  // Declared after the effect above so that on a rebuild the driver exists
  // before this runs — effects fire in declaration order.
  useEffect(() => {
    if (dismissing) driver.current?.collapse();
    else driver.current?.open();
  }, [dismissing]);

  return { surface, shape, path, content };
}
