import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

/**
 * Raises the main window, where Settings lives (the tray's "Open Aloud" does
 * the same from Rust, `tray::show_main`). Done from the webview so the gear
 * needs no command of its own: it costs the overlay two window permissions
 * (capabilities/default.json) instead of Rust.
 */
export async function openSettings() {
  try {
    const main = await WebviewWindow.getByLabel("main");
    await main?.show();
    await main?.setFocus();
  } catch {
    /* browser preview: no windows to raise */
  }
}
