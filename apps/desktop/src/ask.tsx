import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { commands } from "./lib/bindings";
import {
  Conversation,
  ConversationContent,
} from "@/components/ai-elements/conversation";
import { MessageResponse } from "@/components/ai-elements/message";
import { Loader } from "@/components/ai-elements/loader";
import { GlassRefraction } from "./features/glass/GlassRefraction";
import "./index.css";
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

  return (
    <div className="ask">
      <GlassRefraction />
      <header className="ask__header">
        <p className="ask__question">{question}</p>
        <button className="ask__close" onClick={() => commands.closeAsk()}>
          ✕
        </button>
      </header>

      {/* Conversation auto-sticks to the bottom as tokens stream in.
          MessageResponse (Streamdown) renders streaming markdown safely. */}
      <Conversation className="flex-1">
        <ConversationContent className="p-4">
          {state === "failed" ? (
            <p className="text-[#ff9f0a]">{error}</p>
          ) : (
            <>
              <MessageResponse>{answer}</MessageResponse>
              {state === "streaming" && !answer && <Loader />}
            </>
          )}
        </ConversationContent>
      </Conversation>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Ask />
  </React.StrictMode>,
);
