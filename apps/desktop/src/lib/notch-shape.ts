/**
 * The notch silhouette: a panel that flows out of the menu bar rather than
 * hanging off it.
 *
 * The two top corners are *concave*: at the very top the shape is full width
 * and curves inward as it descends, so the sides peel away from the screen
 * edge. A `border-radius` cannot express that — CSS radii only round convex
 * corners — so this is a path, rebuilt from the radii `lib/spring.ts` writes
 * per frame.
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
  // itself — reachable mid-spring, when the panel is briefly narrow.
  const top = Math.max(0, Math.min(topRadius, width / 2));
  const bottom = Math.max(0, Math.min(bottomRadius, Math.max(0, width / 2 - top), height));

  // Quadratic: the control point sits at the corner being cut, which gives the
  // shoulder its fullness. A cubic or an arc reads subtly wrong.
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
