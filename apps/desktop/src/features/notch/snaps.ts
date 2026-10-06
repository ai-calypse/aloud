import type { Snap } from "@/lib/bindings";

/**
 * The window positions, in the order the view lays them out. Here and not in
 * the view so ⌘K's actions and the buttons cannot disagree about what exists.
 */
export const SNAPS: { id: Snap; label: string; group: "Halves" | "Quarters" | "Whole" }[] = [
  { id: "left", label: "Left", group: "Halves" },
  { id: "right", label: "Right", group: "Halves" },
  { id: "top", label: "Top", group: "Halves" },
  { id: "bottom", label: "Bottom", group: "Halves" },
  { id: "topLeft", label: "Top left", group: "Quarters" },
  { id: "topRight", label: "Top right", group: "Quarters" },
  { id: "bottomLeft", label: "Bottom left", group: "Quarters" },
  { id: "bottomRight", label: "Bottom right", group: "Quarters" },
  { id: "maximize", label: "Maximize", group: "Whole" },
  { id: "center", label: "Center", group: "Whole" },
];
