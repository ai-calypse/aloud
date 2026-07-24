import { useEffect, useRef } from "react";

/**
 * The Claude Code launch-banner mascot, ported straight from agent-notch's
 * IndicatorView.drawCrab — same block characters, same 1.6×3.2 sub-cell aspect
 * (terminal cells are ~2× taller than wide, so he doesn't squish), same walk,
 * bob and body shimmer. No profile disc, no backing: a bare transparent canvas,
 * exactly as he walks beside the real notch. The canvas sizes itself to
 * whatever avatar box it takes the class of, from the 20px ambient strip to the
 * full notch card.
 */

// Two frames: the feet alternate so it walks.
const FRAMES: string[][] = [
  [" ▐▛███▜▌ ", "▝▜█████▛▘", "  ▘▘ ▝▝  "],
  [" ▐▛███▜▌ ", "▝▜█████▛▘", "  ▝▝ ▘▘  "],
];

// Each half-block char is four quadrant bits: (upper-left, upper-right,
// lower-left, lower-right). Half of the mascot's charm is these sub-cells.
const QUADRANTS: Record<string, [boolean, boolean, boolean, boolean]> = {
  "█": [true, true, true, true],
  "▐": [false, true, false, true],
  "▌": [true, false, true, false],
  "▛": [true, true, true, false],
  "▜": [true, true, false, true],
  "▙": [true, false, true, true],
  "▟": [false, true, true, true],
  "▘": [true, false, false, false],
  "▝": [false, true, false, false],
  "▖": [false, false, true, false],
  "▗": [false, false, false, true],
  " ": [false, false, false, false],
};

// Anthropic coral, the mascot's colour in agent-notch.
const CORAL = "217, 120, 87";

// agent-notch's sub-cell aspect: a terminal cell is ~2× taller than wide, so a
// sub-cell is 1.6 × 3.2. Keeping the ratio is what stops the critter squishing.
const SUB_W = 1.6;
const SUB_H = 3.2;

// The two padding columns on each side are always blank, so the critter's real
// content spans sub-cell columns 2..15 (14 wide). Fitting to that, not the full
// 18, keeps him from floating in dead space.
const CONTENT_X0 = 2; // first non-blank sub-cell column
const CONTENT_W = 14; // sub-cells of actual critter
const ROWS = FRAMES[0].length * 2; // 6 sub-cells tall

/** The same hash-noise agent-notch uses, so the shimmer looks identical. */
function noise(seed: number): number {
  const n = Math.sin(seed * 0.0001) * 43758.5453;
  return n - Math.floor(n);
}

export function PetMascot({
  className,
  ...rest
}: React.HTMLAttributes<HTMLCanvasElement>) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;

    let box = canvas.clientWidth || 24;
    const ro = new ResizeObserver(() => {
      box = canvas.clientWidth || box;
    });
    ro.observe(canvas);

    let raf = 0;
    const start = performance.now();
    const draw = (now: number) => {
      const px = box * dpr;
      if (canvas.width !== px) {
        canvas.width = px;
        canvas.height = px;
      }
      // The critter's natural box in sub-cell units, then a single scale that
      // fits it into ~90% of the (square) canvas while preserving the aspect.
      const boxW = CONTENT_W * SUB_W;
      const boxH = ROWS * SUB_H;
      const scale = (px * 0.9) / Math.max(boxW, boxH);
      const w = SUB_W * scale;
      const h = SUB_H * scale;
      const ox = (px - boxW * scale) / 2 - CONTENT_X0 * w;
      const oy = (px - boxH * scale) / 2;
      const t = (now - start) / 1000;
      ctx.clearRect(0, 0, px, px);
      const walk = Math.floor(t * 2.5);
      const frame = FRAMES[walk % 2];
      const bob = (walk % 2 === 0 ? -0.5 : 0.5) * h * 0.5;
      const step = Math.floor(t * 3);
      frame.forEach((line, j) => {
        for (let i = 0; i < line.length; i++) {
          const q = QUADRANTS[line[i]];
          if (!q) continue;
          const cells: [boolean, number, number][] = [
            [q[0], 0, 0],
            [q[1], 1, 0],
            [q[2], 0, 1],
            [q[3], 1, 1],
          ];
          for (const [on, qx, qy] of cells) {
            if (!on) continue;
            const r = noise(i * 374761 + j * 668265 + (qx + qy * 2) * 97 + step * 982451);
            const isFeet = j === frame.length - 1;
            const alpha = isFeet ? 1 : 0.8 + 0.2 * r;
            ctx.fillStyle = `rgba(${CORAL}, ${alpha})`;
            ctx.fillRect(
              ox + (i * 2 + qx) * w,
              oy + (j * 2 + qy) * h + bob,
              w - 0.3 * dpr,
              h - 0.4 * dpr,
            );
          }
        }
      });
      raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, []);

  return <canvas ref={ref} className={className} aria-hidden {...rest} />;
}
