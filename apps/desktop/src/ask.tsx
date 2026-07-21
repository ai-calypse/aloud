import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { commands, events } from "./lib/bindings";
import { usePanelAutosize } from "./lib/autosize";
import { PanelShell } from "@/components/ui/panel-shell";
import { Input } from "@/components/ui/input";
import { MessageResponse } from "@/components/ai-elements/message";
import { LoaderCircleIcon } from "lucide-react";
import "./index.css";
import "./styles/panel.css";

/**
 * The Ask answer panel, mirroring the Swift app's AskAnswerPanel: question,
 * divider, streaming markdown answer, and a footer with Copy + close.
 */
function Ask() {
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState("");
  const [state, setState] = useState<"streaming" | "done" | "failed">(
    "streaming",
  );
  const [error, setError] = useState("");
  const [typed, setTyped] = useState("");
  const cardRef = useRef<HTMLDivElement>(null);
  const answerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  usePanelAutosize(cardRef);

  useEffect(() => {
    const unlisten = events.ask.listen(({ payload }) => {
      switch (payload.kind) {
        case "question":
          // A new question resets the panel; it is reused, not recreated.
          setQuestion(payload.value);
          setTyped("");
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

  // DOM focus only, never a window focus request: `ask::show` already makes
  // the panel key when it appears, and it is the only thing that takes focus
  // from the app being dictated into. This just puts the caret somewhere
  // useful inside a window that already has the keyboard.
  //
  // In an effect rather than in the event handler because PanelShell is keyed
  // on the question — a new one remounts the input, and focusing before that
  // render lands on the node about to be discarded.
  useEffect(() => {
    if (question) inputRef.current?.focus();
  }, [question]);

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
      footer={
        <Input
          ref={inputRef}
          className="h-8 flex-1 rounded-full text-ui"
          // Not "Ask a follow-up": `stream_answer` builds a fresh
          // [system, user] pair with no prior turn, so there is no
          // conversation to follow up on. Promising one and then answering
          // "and what about Friday?" with no context is worse than not
          // offering it.
          placeholder="Ask a question…"
          value={typed}
          // Disabled mid-answer: two streams write Delta into the same window,
          // and the second one's Question event resets `answer` to "" while the
          // first keeps appending — the two answers splice together.
          disabled={state === "streaming"}
          onChange={(e) => setTyped(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== "Enter" || !typed.trim() || state === "streaming")
              return;
            // Cleared here rather than waiting for the backend's `question`
            // event to echo back: until that lands the text sits in the field
            // and is trivially re-submittable.
            const question = typed;
            setTyped("");
            // The same backend entry point the spoken trigger uses, so the
            // two paths cannot answer differently.
            void commands.askText(question);
          }}
        />
      }
    >
      {state === "failed" ? (
        <p className="m-0 text-destructive">{error}</p>
      ) : answer ? (
        <MessageResponse>{answer}</MessageResponse>
      ) : (
        <LoaderCircleIcon className="size-4 animate-spin" />
      )}
    </PanelShell>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Ask />
  </React.StrictMode>,
);
