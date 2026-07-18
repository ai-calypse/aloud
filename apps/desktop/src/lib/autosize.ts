import { useEffect } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

/** Matches the Swift panel's fixed width and the width in tauri.conf.json. */
export const PANEL_WIDTH = 500;

/** Chrome around the answer scroll area (question, divider, footer,
 * paddings), mirroring the Swift panel's fixed worst-case estimate. */
const CHROME_HEIGHT = 150;

/**
 * Grows the window downward to fit the card, voice-dict style: Rust anchors
 * the top edge once at show, then height follows content. Also publishes
 * `--answer-cap` (55% of the screen minus chrome) so the answer area scrolls
 * instead of growing past it.
 */
export function usePanelAutosize(ref: React.RefObject<HTMLDivElement | null>) {
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    document.documentElement.style.setProperty(
      "--answer-cap",
      `${Math.round(screen.availHeight * 0.55 - CHROME_HEIGHT)}px`,
    );
    const win = getCurrentWindow();
    const observer = new ResizeObserver(() => {
      const height = Math.max(60, Math.ceil(el.getBoundingClientRect().height));
      void win.setSize(new LogicalSize(PANEL_WIDTH, height));
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
}
