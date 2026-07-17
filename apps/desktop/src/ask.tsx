import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import Markdown from "react-markdown";
import { commands } from "./lib/bindings";
import "./ask.css";

/** Mirrors the Rust `ask::AskEvent`. */
type AskEvent =
  | { kind: "question"; value: string }
  | { kind: "delta"; value: string }
  | { kind: "done" }
  | { kind: "failed"; value: string };

/**
 * The Ask answer panel. Rust streams tokens in; this renders them as markdown
 * as they arrive.
 */
function Ask() {
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState("");
  const [state, setState] = useState<"streaming" | "done" | "failed">("streaming");
  const [error, setError] = useState("");
  const body = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const unlisten = listen<AskEvent>("ask", ({ payload }) => {
      switch (payload.kind) {
        case "question":
          // A new question resets the panel; it is reused, not recreated.
          setQuestion(payload.value);
          setAnswer("");
          setError("");
          setState("streaming");
          break;
        case "delta":
          setAnswer((a) => a + payload.value);
          break;
        case "done":
          setState("done");
          break;
        case "failed":
          setError(payload.value);
          setState("failed");
          break;
      }
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") commands.closeAsk();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Follow the answer as it streams, unless the user has scrolled up to read.
  useEffect(() => {
    const el = body.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    if (atBottom) el.scrollTop = el.scrollHeight;
  }, [answer]);

  return (
    <div className="ask">
      <header className="ask__header">
        <p className="ask__question">{question}</p>
        <button className="ask__close" onClick={() => commands.closeAsk()}>
          ✕
        </button>
      </header>

      <div className="ask__body" ref={body}>
        {state === "failed" ? (
          <p className="ask__error">{error}</p>
        ) : (
          <Markdown>{answer}</Markdown>
        )}
        {state === "streaming" && <span className="ask__caret" />}
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Ask />
  </React.StrictMode>,
);
