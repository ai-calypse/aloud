import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { commands } from "./lib/bindings";
import { Button } from "@/components/ui/button";
import { Loader } from "@/components/ai-elements/loader";
import { GlassRefraction } from "./features/glass/GlassRefraction";
import "./index.css";
import "./agent.css";

/** Mirrors the Rust `agent::ActionSummary`. */
type ActionSummary = {
  action: string;
  app: string;
  details: string[];
};

/** Mirrors the Rust `agent::AgentEvent`. */
type AgentEvent =
  | { kind: "heard"; value: string }
  | { kind: "confirm"; value: ActionSummary }
  | { kind: "status"; value: string }
  | { kind: "done"; value: string }
  | { kind: "failed"; value: string };

type Phase =
  | { name: "thinking" }
  | { name: "confirm"; action: ActionSummary }
  | { name: "status"; message: string }
  | { name: "done"; message: string }
  | { name: "failed"; message: string };

/**
 * The agent panel. Shows what was heard, asks to confirm a write before it
 * fires, then reports the result. The confirm gate is the whole point: a write
 * never runs until Yes is pressed here.
 */
function Agent() {
  const [heard, setHeard] = useState("");
  const [phase, setPhase] = useState<Phase>({ name: "thinking" });

  useEffect(() => {
    const unlisten = listen<AgentEvent>("agent", ({ payload }) => {
      switch (payload.kind) {
        case "heard":
          setHeard(payload.value);
          setPhase({ name: "thinking" });
          break;
        case "confirm":
          setPhase({ name: "confirm", action: payload.value });
          break;
        case "status":
          setPhase({ name: "status", message: payload.value });
          break;
        case "done":
          setPhase({ name: "done", message: payload.value });
          break;
        case "failed":
          setPhase({ name: "failed", message: payload.value });
          break;
      }
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Escape cancels: a pending confirmation reads as No, so nothing fires.
      if (e.key === "Escape") commands.closeAgent();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="agent">
      <GlassRefraction />
      <header className="agent__header">
        <p className="agent__heard">{heard}</p>
        <button className="agent__close" onClick={() => commands.closeAgent()}>
          ✕
        </button>
      </header>

      <div className="flex-1 overflow-y-auto p-4">
        {phase.name === "thinking" && (
          <div className="flex items-center gap-2 text-[#a1a1a6]">
            <Loader />
            Understanding…
          </div>
        )}

        {phase.name === "confirm" && (
          <div className="flex flex-col gap-3">
            <p className="text-[15px] font-semibold">{phase.action.action}</p>
            <dl className="flex flex-col gap-1.5 rounded-lg bg-white/5 p-3 text-[12.5px]">
              {phase.action.details.map((line) => {
                const [key, ...rest] = line.split(": ");
                return (
                  <div key={line} className="grid grid-cols-[6rem_1fr] gap-2">
                    <dt className="text-[#a1a1a6]">{key}</dt>
                    <dd className="m-0">{rest.join(": ")}</dd>
                  </div>
                );
              })}
            </dl>
            {/* The confirm gate: nothing fires until "Do it" is pressed. */}
            <div className="flex justify-end gap-2">
              <Button
                variant="secondary"
                size="sm"
                onClick={() => commands.confirmAgent(false)}
              >
                Cancel
              </Button>
              <Button
                size="sm"
                onClick={() => commands.confirmAgent(true)}
                autoFocus
              >
                Do it
              </Button>
            </div>
          </div>
        )}

        {phase.name === "status" && (
          <div className="flex items-center gap-2 text-[#a1a1a6]">
            <Loader />
            {phase.message}
          </div>
        )}

        {phase.name === "done" && (
          <p className="text-[#30d158]">{phase.message}</p>
        )}

        {phase.name === "failed" && (
          <p className="text-[#ff9f0a]">{phase.message}</p>
        )}
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Agent />
  </React.StrictMode>,
);
