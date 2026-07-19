import React, { useEffect, useState } from "react";
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

// A waveform envelope: shorter at the edges, tallest in the middle, so the
// bars read as a voice level rather than an even row. Values are peak scaleY.
const BAR_PEAKS = [0.55, 0.8, 1, 1, 0.8, 0.55];

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
  // null means "no level has arrived yet", which is what the canned-animation
  // fallback keys off — distinct from a real level of 0 during silence.
  const [level, setLevel] = useState<number | null>(null);
  const [download, setDownload] = useState<number | null>(null);

  useEffect(() => {
    const unlistenPhase = events.dictationPhase.listen((e) => {
      setPhase(e.payload);
      // A new run clears the last run's message and stale level.
      if (e.payload === "recording") {
        setError(null);
      } else if (e.payload !== "error") {
        setLevel(null);
      }
    });
    const unlistenLevel = events.audioLevel.listen((e) => setLevel(e.payload ?? 0));
    const unlistenError = events.overlayError.listen((e) => setError(e.payload));
    // First use of the local engine fetches ~150MB; without this the pill sits
    // on "Transcribing…" for minutes and the app looks hung.
    const unlistenDownload = events.modelDownload.listen((e) =>
      setDownload(e.payload === null || e.payload >= 1 ? null : e.payload),
    );
    return () => {
      unlistenPhase.then((off) => off());
      unlistenLevel.then((off) => off());
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
        <span
          // No level yet (the first frames of a session) sits at mid-scale
          // rather than collapsed, so the pill never appears as a flat row.
          className="bars"
          style={{ "--level": level ?? 0.5 } as React.CSSProperties}
          aria-hidden
        >
          {/* Staggered + a per-bar peak so the row reads as a waveform. */}
          {BAR_PEAKS.map((peak, i) => (
            <i
              key={i}
              style={
                {
                  animationDelay: `${i * 0.1}s`,
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
