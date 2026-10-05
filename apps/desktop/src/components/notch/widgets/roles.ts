/**
 * What a widget is about, which decides its accent (`[data-w-role]` in
 * tokens.css). Roles are meaning, not decoration: pink is music, a draining
 * red ring is time, green is a healthy charge.
 */
export const WIDGET_ROLES = [
  "calendar",
  "ok",
  "media",
  "podcast",
  "move",
  "exercise",
  "stand",
  "blue",
  "cyan",
  "orange",
  "sleep",
  "sun",
  "info",
  "agent",
] as const;

export type WidgetRole = (typeof WIDGET_ROLES)[number];

/** The calendar colours an event card knows (`[data-w-event]`). */
export type EventTone = "green" | "yellow" | "red" | "blue";
