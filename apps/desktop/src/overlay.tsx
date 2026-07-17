import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import "./overlay.css";

/** Mirrors the Rust `pipeline::Phase`. */
type Phase = "idle" | "recording" | "transcribing" | "done" | "error";

const LABEL: Record<Phase, string> = {
  idle: "",
  recording: "Listening",
  transcribing: "Transcribing",
  done: "Pasted",
  error: "Failed",
};

/**
 * The recording pill. Rust drives the window's visibility, so this only ever
 * renders the current phase.
 */
function Overlay() {
  const [phase, setPhase] = useState<Phase>("idle");

  useEffect(() => {
    const unlisten = listen<Phase>("dictation-phase", (e) => setPhase(e.payload));
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  return (
    <div className={`pill pill--${phase}`}>
      {phase === "recording" ? (
        <span className="bars" aria-hidden>
          {/* Staggered so the bars read as a waveform, not a blink. */}
          {[0, 1, 2, 3].map((i) => (
            <i key={i} style={{ animationDelay: `${i * 0.12}s` }} />
          ))}
        </span>
      ) : (
        <span className={`dot dot--${phase}`} aria-hidden />
      )}
      <span className="label">{LABEL[phase]}</span>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
