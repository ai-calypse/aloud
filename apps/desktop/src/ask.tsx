import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { commands, events } from "./lib/bindings";
import { usePanelAutosize } from "./lib/autosize";
import { PanelShell } from "@/components/ui/panel-shell";
import { MessageResponse } from "@/components/ai-elements/message";
import { Loader } from "@/components/ai-elements/loader";
import "./index.css";
import "./styles/panel.css";

/**
 * The Ask answer panel, mirroring the Swift app's AskAnswerPanel: question,
 * divider, streaming markdown answer, and a footer with Copy + close.
 */
function Ask() {
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState("");
  const [state, setState] = useState<"streaming" | "done" | "failed">("streaming");
  const [error, setError] = useState("");
  const cardRef = useRef<HTMLDivElement>(null);
  const answerRef = useRef<HTMLDivElement>(null);

  usePanelAutosize(cardRef);

  useEffect(() => {
    const unlisten = events.ask.listen(({ payload }) => {
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

  // Follow the stream to the bottom, and stop once it finishes — the same
  // simpler-than-tracking-manual-scroll choice the Swift panel made.
  useEffect(() => {
    const el = answerRef.current;
    if (el && state === "streaming") el.scrollTop = el.scrollHeight;
  }, [answer, state]);

  return (
    <PanelShell
      // Keyed on the question so a new one resets the copied state.
      key={question}
      title={question}
      cardRef={cardRef}
      bodyRef={answerRef}
      copyText={answer}
      onClose={() => commands.closeAsk()}
    >
      {state === "failed" ? (
        <p className="m-0 text-destructive">{error}</p>
      ) : answer ? (
        <MessageResponse>{answer}</MessageResponse>
      ) : (
        <Loader />
      )}
    </PanelShell>
  );
}


ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Ask />
  </React.StrictMode>,
);
