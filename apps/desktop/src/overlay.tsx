import React, { useEffect, useRef, useState, type ReactNode } from "react";
import ReactDOM from "react-dom/client";
import { events, type OverlayStyle, type Phase } from "./lib/bindings";
import "./styles/overlay.css";

const LABEL: Record<Phase, string> = {
  idle: "",
  recording: "Listening…",
  transcribing: "Transcribing…",
  done: "Pasted",
  // Replaced by the real reason as soon as Rust sends one; this is only what
  // shows if an error somehow arrives without a message.
  error: "Failed",
};

/** A long error still has to fit one line on a small pill. */
const ERROR_CHARS = 90;

/**
 * Bars in the listening wave.
 *
 * Five, as the Swift app used. The count and the per-bar phase step together
 * set how much of a wave fits across the row (see the delay in overlay.css) —
 * change this and that has to move with it.
 */
const BAR_COUNT = 5;

/**
 * Per-bar height ceiling: a raised cosine, tallest in the middle.
 *
 * Without it every bar reaches the same height and the row reads as a ribbon
 * rather than a waveform. Computed rather than hand-tuned so the shape holds
 * if BAR_COUNT changes.
 *
 * The floor is 0.6, not 0, because this multiplies through the wave's trough
 * and the silent-level scale as well: a shallower envelope crushed the outer
 * bars to a 1px dash at rest, which read as a broken row of dots rather than
 * as short bars.
 */
const BAR_PEAKS = Array.from({ length: BAR_COUNT }, (_, i) => {
  const position = i / (BAR_COUNT - 1); // 0…1 across the row
  return 0.6 + 0.4 * Math.sin(Math.PI * position);
});



/**
 * The recording pill. Rust drives the window's visibility, so this only ever
 * renders the current phase.
 */
function Overlay() {
  // Dev/QA: `?phase=recording` renders the pill statically in a browser so it
  // can be seen and screenshotted without holding the dictate hotkey. In the
  // real app there is no query string, so this is idle until Rust drives it.
  const preview = new URLSearchParams(location.search).get("phase") as Phase | null;
  const [phase, setPhase] = useState<Phase>(preview ?? "idle");
  // Rust picks the style and pushes it on every show — including the swap back
  // to the pill for an error, which no wing is wide enough to show.
  const [style, setStyle] = useState<OverlayStyle>(
    (new URLSearchParams(location.search).get("style") as OverlayStyle) ??
      "liquidGlass",
  );
  const [error, setError] = useState<string | null>(null);
  const [download, setDownload] = useState<number | null>(null);
  // The root, not the bar row: --level inherits, so the wings style can draw
  // two waves off one write.
  const root = useRef<HTMLDivElement>(null);

  // Levels land every 50ms. Writing --level straight onto the node keeps them
  // out of React entirely: at 20 renders a second the pill would rebuild its
  // subtree while the compositor is mid-wave, which is exactly the stutter the
  // CSS-owned animation exists to avoid. Null (no signal yet) reads as silence.
  useEffect(() => {
    const unlisten = events.audioLevel.listen((e) => {
      const level = Math.min(1, Math.max(0, e.payload ?? 0));
      root.current?.style.setProperty("--level", String(level));
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  useEffect(() => {
    const unlistenPhase = events.dictationPhase.listen((e) => {
      setPhase(e.payload);
      // A new run clears the last run’s message.
      if (e.payload === "recording") setError(null);
    });
    const unlistenError = events.overlayError.listen((e) => setError(e.payload));
    const unlistenStyle = events.overlayStyle.listen((e) => {
      setStyle(e.payload.style);
      // The exact notch cutout, measured natively, so the wings weld to it
      // rather than straddle a fixed guess. Written as vars the CSS reads;
      // zero for a pill, where the wave falls back to its own gap/height.
      root.current?.style.setProperty("--notch-w", `${e.payload.notchWidth ?? 0}px`);
      root.current?.style.setProperty(
        "--notch-h",
        `${e.payload.notchHeight ?? 0}px`,
      );
    });
    // First use of the local engine fetches ~150MB; without this the pill sits
    // on "Transcribing…" for minutes and the app looks hung.
    const unlistenDownload = events.modelDownload.listen((e) =>
      setDownload(e.payload === null || e.payload >= 1 ? null : e.payload),
    );
    return () => {
      unlistenPhase.then((off) => off());
      unlistenError.then((off) => off());
      unlistenStyle.then((off) => off());
      unlistenDownload.then((off) => off());
    };
  }, []);

  // The live transcript is no longer shown here — it has its own panel. The
  // pill is back to a status word plus the wave.
  const label =
    phase === "error" && error
      ? truncate(error, ERROR_CHARS)
      : download !== null
        ? `Downloading model… ${Math.round(download * 100)}%`
        : LABEL[phase];

  return (
    <div className="overlay-root" ref={root}>
    {/* Refraction filter for the glass pill: turbulence drives a displacement
        map that warps the glass highlight layer. WebKit renders this on an
        element's own content (unlike backdrop-filter: url(), which Safari does
        not support), so it works in the Tauri webview. */}
    {style === "liquidGlass" && (
      <svg width="0" height="0" aria-hidden style={{ position: "absolute" }}>
        <filter id="pill-refraction" x="-20%" y="-20%" width="140%" height="140%">
          <feTurbulence
            type="fractalNoise"
            baseFrequency="0.012 0.02"
            numOctaves={2}
            seed={7}
            result="noise"
          />
          <feGaussianBlur in="noise" stdDeviation="1.2" result="softNoise" />
          <feDisplacementMap
            in="SourceGraphic"
            in2="softNoise"
            scale={22}
            xChannelSelector="R"
            yChannelSelector="G"
          />
        </filter>
      </svg>
    )}
    {style === "wings" ? (
      // voice-dict's minimalist overlay: one continuous black bar welded to the
      // notch — the waveform on the left, a recording light on the right, the
      // measured cutout between them. Only ever shown on a screen that has a
      // notch; Rust falls back to a pill otherwise (see overlay.rs).
      <div className={`wings wings--${phase}`}>
        <span className="wing wing--left">
          <PhaseSwap phase={phase}>
            <Indicator phase={phase} />
          </PhaseSwap>
        </span>
        {/* The notch itself: solid black, exactly the cutout's width, so the
            bar is continuous across it and the camera notch disappears into
            the strip — the whole point of the blend. */}
        <span className="wing-notch" aria-hidden />
        <span className="wing wing--right">
          {phase === "recording" && (
            <span className="dot dot--recording" aria-hidden />
          )}
        </span>
      </div>
    ) : style === "pill" ? (
      // voice-dict's drop-down pill: a solid-black panel welded to the top,
      // rounded only along the bottom, with the wave and status sitting just
      // below the notch (the __notch spacer is the measured cutout height).
      <div className={`drop-pill drop-pill--${phase}`}>
        <span className="drop-pill__notch" aria-hidden />
        <div className="drop-pill__body">
          <PhaseSwap phase={phase}>
            <Indicator phase={phase} />
            <span className="label" title={error ?? undefined}>
              {label}
            </span>
          </PhaseSwap>
        </div>
      </div>
    ) : (
      // The liquid-glass pill: a refractive capsule below the menu bar, with
      // the warped sheen (below) riding over the backdrop blur.
      <div className={`pill pill--${phase}`}>
        <span className="pill__refraction" aria-hidden />
        <PhaseSwap phase={phase}>
          <Indicator phase={phase} />
          <span className="label" title={error ?? undefined}>
            {label}
          </span>
        </PhaseSwap>
      </div>
    )}
    </div>
  );
}

/**
 * Cross-fades its contents whenever the phase changes.
 *
 * The `key` is the whole mechanism: React remounts on a new phase, so the CSS
 * entrance in overlay.css runs again without the usual reflow-to-restart-a-
 * keyframe dance. Cheap here because the subtree is a dot (or five bars) and a
 * word — and the bars only exist in one phase, so nothing mid-wave is thrown
 * away by the remount.
 */
function PhaseSwap({ phase, children }: { phase: Phase; children: ReactNode }) {
  return (
    <span key={phase} className="phase-swap">
      {children}
    </span>
  );
}

/** The wave while recording, a status dot otherwise. Shared by both styles. */
function Indicator({ phase }: { phase: Phase }) {
  if (phase !== "recording") {
    return <span className={`dot dot--${phase}`} aria-hidden />;
  }
  // A continuous wave the voice scales rather than drives. CSS owns the
  // motion; these two custom properties only give each bar its place in the
  // row and its height ceiling.
  return (
    <span className="bars" aria-hidden>
      {BAR_PEAKS.map((peak, i) => (
        <i key={i} style={{ "--i": i, "--peak": peak } as React.CSSProperties} />
      ))}
    </span>
  );
}

/** Cuts on a char boundary so a multi-byte character is never split. */
function truncate(text: string, max: number): string {
  const flat = text.replace(/\s+/g, " ").trim();
  return [...flat].length <= max ? flat : `${[...flat].slice(0, max).join("")}…`;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
