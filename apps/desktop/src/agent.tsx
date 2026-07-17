import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { commands } from "./lib/bindings";
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
      <header className="agent__header">
        <p className="agent__heard">{heard}</p>
        <button className="agent__close" onClick={() => commands.closeAgent()}>
          ✕
        </button>
      </header>

      <div className="agent__body">
        {phase.name === "thinking" && (
          <p className="agent__status">Understanding…</p>
        )}

        {phase.name === "confirm" && (
          <div className="agent__confirm">
            <p className="agent__action">{phase.action.action}</p>
            <dl className="agent__details">
              {phase.action.details.map((line) => {
                const [key, ...rest] = line.split(": ");
                return (
                  <div key={line} className="agent__detail">
                    <dt>{key}</dt>
                    <dd>{rest.join(": ")}</dd>
                  </div>
                );
              })}
            </dl>
            <div className="agent__actions">
              <button
                className="agent__btn agent__btn--cancel"
                onClick={() => commands.confirmAgent(false)}
              >
                Cancel
              </button>
              <button
                className="agent__btn agent__btn--go"
                onClick={() => commands.confirmAgent(true)}
                autoFocus
              >
                Do it
              </button>
            </div>
          </div>
        )}

        {phase.name === "status" && (
          <p className="agent__status">{phase.message}</p>
        )}

        {phase.name === "done" && (
          <p className="agent__done">{phase.message}</p>
        )}

        {phase.name === "failed" && (
          <p className="agent__error">{phase.message}</p>
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
