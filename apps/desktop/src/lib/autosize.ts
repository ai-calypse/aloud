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
    // The panels key their card on the question/run, so React replaces the card
    // node on every new one. This effect runs once (the ref is stable), so an
    // observer on the card would be left watching a detached node — the window
    // then never grows past the first question and the answer is clipped.
    // Observe the card's parent (#root, unstyled and never remounted); its
    // height tracks whichever card is currently mounted inside it.
    const tracked = el.parentElement ?? el;
    // Absent outside a Tauri window — a browser preview, or a webview that has
    // not finished attaching. Sizing is an enhancement; throwing here would
    // take the whole panel down with it, which is a poor trade for a window
    // that is already the right width.
    let win: ReturnType<typeof getCurrentWindow> | null = null;
    try {
      win = getCurrentWindow();
    } catch {
      return;
    }

    const observer = new ResizeObserver(() => {
      const height = Math.max(
        60,
        Math.ceil(tracked.getBoundingClientRect().height),
      );
      void win?.setSize(new LogicalSize(PANEL_WIDTH, height));
    });
    observer.observe(tracked);
    return () => observer.disconnect();
  }, [ref]);
}
