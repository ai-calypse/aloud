import type { AgentEvent, ListenState } from "./bindings";

/**
 * What a run's event says about where it is, for the listening bar's status
 * line; `undefined` leaves the line as it was. Mirrors Jev Voice's
 * `CommandBarView.status`: the current action while working, then how it
 * ended next to whether the mic is still on.
 */
export function stepFor(event: AgentEvent): string | undefined {
  switch (event.kind) {
    case "runStarted":
      return "";
    case "status":
      return event.value;
    case "asked":
      return "Answer in the panel";
    case "runFinished":
      return firstLine(event.value) || "Done";
    case "runError":
      return firstLine(event.value);
    default:
      return undefined;
  }
}

export function barStatus(state: ListenState, step: string): string {
  if (state.problem) return state.problem;
  if (state.busy) return step || "Working…";
  const mic = state.mic ? "Listening" : "Mic off";
  return step ? `${step} · ${mic}` : mic;
}

function firstLine(text: string): string {
  return text.split("\n")[0].trim();
}
