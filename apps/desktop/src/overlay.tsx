import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { events, type Phase } from "./lib/bindings";
import "./overlay.css";

const LABEL: Record<Phase, string> = {
  idle: "",
  recording: "Listening…",
  transcribing: "Transcribing…",
  done: "Pasted",
  error: "Failed",
};

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

  useEffect(() => {
    const unlisten = events.dictationPhase.listen((e) => setPhase(e.payload));
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

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
        <span className="bars" aria-hidden>
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
      <span className="label">{LABEL[phase]}</span>
    </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
