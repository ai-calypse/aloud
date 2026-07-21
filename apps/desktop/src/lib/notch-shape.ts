/**
 * The notch silhouette: a panel that flows out of the menu bar rather than
 * hanging off it.
 *
 * The two top corners are *concave*. That is the whole difference between this
 * and a rounded rectangle, and it is what makes the panel read as an extension
 * of the cutout instead of a card parked underneath it: at the very top the
 * shape is full width, and it curves inward as it descends, so the sides appear
 * to peel away from the screen edge. A `border-radius` cannot express it — CSS
 * radii only ever round convex corners — which is why this is a path.
 *
 * Ported from the reference implementation's `NotchShape`, which drives the two
 * radii through SwiftUI's `animatableData` so the geometry interpolates rather
 * than snapping between states. `lib/spring.ts` is the equivalent here: it
 * writes `topRadius`/`bottomRadius` per frame and this rebuilds `d`.
 *
 *              ┌──0,0                    w,0──┐
 *        top ↘ │  ╰─╮                  ╭─╯    │ ↙ concave
 *   radius     │    │                  │      │
 *              │    ╰──────────────────╯      │ ↙ convex, bottom radius
 */
export function notchPath(
  width: number,
  height: number,
  topRadius: number,
  bottomRadius: number,
): string {
  // A radius wider than half the box inverts the curve and the path crosses
  // itself. Mid-spring the panel is briefly narrow, so this is reachable in
  // normal use, not just from bad input.
  const top = Math.max(0, Math.min(topRadius, width / 2));
  const bottom = Math.max(0, Math.min(bottomRadius, Math.max(0, width / 2 - top), height));

  // Quadratic curves, matching the reference exactly: the control point sits at
  // the corner the curve is cutting, which is what gives the shoulder its
  // particular fullness. A cubic or an arc here reads subtly wrong.
  return [
    `M 0 0`,
    `Q ${top} 0 ${top} ${top}`,
    `L ${top} ${height - bottom}`,
    `Q ${top} ${height} ${top + bottom} ${height}`,
    `L ${width - top - bottom} ${height}`,
    `Q ${width - top} ${height} ${width - top} ${height - bottom}`,
    `L ${width - top} ${top}`,
    `Q ${width - top} 0 ${width} 0`,
    `Z`,
  ].join(" ");
}
