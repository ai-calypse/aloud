import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { events, type Phase } from "./lib/bindings";
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

/** Bars in the listening wave. */
const BAR_COUNT = 14;

/**
 * Per-bar height ceiling: a raised cosine, tallest in the middle.
 *
 * Without it every bar reaches the same height and the row reads as a ribbon
 * rather than a waveform. Computed rather than hand-tuned so the shape holds
 * if BAR_COUNT changes.
 */
const BAR_PEAKS = Array.from({ length: BAR_COUNT }, (_, i) => {
  const position = i / (BAR_COUNT - 1); // 0…1 across the row
  return 0.45 + 0.55 * Math.sin(Math.PI * position);
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
  const [error, setError] = useState<string | null>(null);
  const [download, setDownload] = useState<number | null>(null);
  const bars = useRef<HTMLSpanElement>(null);

  // Levels land every 50ms. Writing --level straight onto the node keeps them
  // out of React entirely: at 20 renders a second the pill would rebuild its
  // subtree while the compositor is mid-wave, which is exactly the stutter the
  // CSS-owned animation exists to avoid. Null (no signal yet) reads as silence.
  useEffect(() => {
    const unlisten = events.audioLevel.listen((e) => {
      const level = Math.min(1, Math.max(0, e.payload ?? 0));
      bars.current?.style.setProperty("--level", String(level));
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  useEffect(() => {
    const unlistenPhase = events.dictationPhase.listen((e) => {
      setPhase(e.payload);
      // A new run clears the last run’s message.
      if (e.payload === "recording") {
        setError(null);
      }
    });
    const unlistenError = events.overlayError.listen((e) => setError(e.payload));
    // First use of the local engine fetches ~150MB; without this the pill sits
    // on "Transcribing…" for minutes and the app looks hung.
    const unlistenDownload = events.modelDownload.listen((e) =>
      setDownload(e.payload === null || e.payload >= 1 ? null : e.payload),
    );
    return () => {
      unlistenPhase.then((off) => off());
      unlistenError.then((off) => off());
      unlistenDownload.then((off) => off());
    };
  }, []);

  const label =
    phase === "error" && error
      ? truncate(error, ERROR_CHARS)
      : download !== null
        ? `Downloading model… ${Math.round(download * 100)}%`
        : LABEL[phase];

  return (
    <div className="overlay-root">
    {/* Refraction filter: turbulence drives a displacement map that warps the
        glass highlight layer. WebKit renders this on an element's own content
        (unlike backdrop-filter: url(), which Safari does not support), so it
        works in the Tauri webview. */}
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
    <div className={`pill pill--${phase}`}>
      {/* Warped glass highlight sitting over the real backdrop blur. */}
      <span className="pill__refraction" aria-hidden />
      {phase === "recording" ? (
        // A continuous wave the voice scales rather than drives. CSS owns the
        // motion; these two custom properties only give each bar its place in
        // the row and its height ceiling.
        <span className="bars" ref={bars} aria-hidden>
          {BAR_PEAKS.map((peak, i) => (
            <i
              key={i}
              style={
                {
                  "--i": i,
                  "--peak": peak,
                } as React.CSSProperties
              }
            />
          ))}
        </span>
      ) : (
        <span className={`dot dot--${phase}`} aria-hidden />
      )}
      <span className="label" title={error ?? undefined}>
        {label}
      </span>
    </div>
    </div>
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
