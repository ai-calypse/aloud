import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { CheckIcon, CopyIcon, XIcon } from "lucide-react";
import { commands, events } from "./lib/bindings";
import { usePanelAutosize } from "./lib/autosize";
import { Card, CardContent, CardFooter, CardHeader } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { MessageResponse } from "@/components/ai-elements/message";
import { Loader } from "@/components/ai-elements/loader";
import "./index.css";
import "./panel.css";

/**
 * The Ask answer panel, mirroring the Swift app's AskAnswerPanel: question,
 * divider, streaming markdown answer, and a footer with Copy + close.
 */
function Ask() {
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState("");
  const [state, setState] = useState<"streaming" | "done" | "failed">("streaming");
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);
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
    setCopied(false);
    const el = answerRef.current;
    if (el && state === "streaming") el.scrollTop = el.scrollHeight;
  }, [answer, state]);

  return (
    <Card ref={cardRef} className="panel gap-3 rounded-2xl border-white/10 py-4 shadow-none">
      <CardHeader className="px-4">
        <p
          className="m-0 line-clamp-2 text-[13px] font-medium text-muted-foreground"
          data-tauri-drag-region
        >
          {question}
        </p>
      </CardHeader>
      <Separator className="bg-white/10" />

      <CardContent className="panel__answer px-4 text-[15px] leading-normal" ref={answerRef}>
        {state === "failed" ? (
          <p className="m-0 text-destructive">{error}</p>
        ) : answer ? (
          <MessageResponse>{answer}</MessageResponse>
        ) : (
          <Loader />
        )}
      </CardContent>

      <CardFooter className="gap-2.5 px-4">
        <span className="flex-1" />
        <Button
          variant="secondary"
          size="sm"
          className="rounded-full"
          onClick={() =>
            navigator.clipboard.writeText(answer).then(() => setCopied(true))
          }
        >
          {copied ? <CheckIcon /> : <CopyIcon />}
          {copied ? "Copied" : "Copy"}
        </Button>
        <Button
          variant="secondary"
          size="icon-sm"
          className="rounded-full"
          aria-label="Close"
          onClick={() => commands.closeAsk()}
        >
          <XIcon />
        </Button>
      </CardFooter>
    </Card>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Ask />
  </React.StrictMode>,
);
