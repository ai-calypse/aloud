import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { events } from "./lib/bindings";
import { usePanelAutosize } from "./lib/autosize";
import "./index.css";
import "./styles/panel.css";

/**
 * The streaming-transcript panel: a wide, growing card that shows the whole
 * live transcript while the dictate key is held, so the words are legible in a
 * way the pill's single clipped line never was.
 *
 * Feedback only, exactly like the pill — Rust never pastes this text as it
 * arrives (whisper revises earlier chunks). The window is click-through and
 * never takes focus, so it cannot steal keystrokes from the app being dictated
 * into. Rust drives visibility; this only ever renders the latest partial.
 */
function Transcript() {
  const [text, setText] = useState("");
  const cardRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  usePanelAutosize(cardRef);

  useEffect(() => {
    // Rust sends an empty partial on show to clear the last run's text, so no
    // separate reset event is needed.
    const unlisten = events.dictationPartial.listen((e) => setText(e.payload));
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  // Keep the newest words in view as the transcript grows past the cap.
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [text]);

  return (
    <div
      ref={cardRef}
      className="panel rounded-2xl border border-white/10 px-5 py-4 text-white/90 shadow-lg"
    >
      <div
        ref={scrollRef}
        className="max-h-[var(--answer-cap,400px)] overflow-y-auto whitespace-pre-wrap text-[15px] leading-relaxed"
      >
        {text || <span className="text-white/40">Listening…</span>}
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Transcript />
  </React.StrictMode>,
);
