import { useEffect, useRef, useState } from "react";
import { ArrowUp, AudioWaveform, MicOff, Square } from "lucide-react";
import { commands, events, type ListenState } from "@/lib/bindings";
import { barStatus, stepFor } from "@/lib/listen-status";
import { cn } from "@/lib/utils";

const INITIAL: ListenState = { mic: false, transcript: "", busy: false, queued: 0, problem: null };

/**
 * The always-listening bar: Jev Voice's `CommandBarView` (Sources/Views.swift),
 * ported as is — mic toggle, what was heard, one status line, send and stop.
 * The window is the bar's exact size, so the whole surface is draggable.
 */
export function ListenBar() {
  const [state, setState] = useState(INITIAL);
  const [step, setStep] = useState("");
  const [typed, setTyped] = useState("");
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    void commands.listenState().then(setState);
    const offs = [
      events.listenState.listen((e) => setState(e.payload)),
      events.agent.listen((e) => {
        const next = stepFor(e.payload);
        if (next !== undefined) setStep(next);
      }),
    ];
    return () => {
      for (const off of offs) void off.then((f) => f());
    };
  }, []);

  const heard = state.transcript !== "";
  const prompt = heard
    ? state.transcript
    : state.mic
      ? "Say or type a command…"
      : "Type a command, or turn the mic on…";
  const hasTyped = typed.trim() !== "";

  function submit() {
    if (!hasTyped) return;
    void commands.listenSubmit(typed);
    setTyped("");
    input.current?.blur();
  }

  return (
    <div
      data-tauri-drag-region
      className="flex h-screen w-screen items-center gap-[11px] rounded-bar border border-bar-stroke bg-bar px-3 py-2.5 text-ink"
    >
      <button
        type="button"
        onClick={() => void commands.listenToggleMic()}
        aria-label={state.mic ? "Turn mic off" : "Turn mic on"}
        title="Toggle listening"
        className={cn(
          "flex h-9 w-[34px] shrink-0 items-center justify-center rounded-bar-key",
          state.mic ? "bg-bar-accent-wash text-bar-accent" : "bg-bar-off-wash text-bar-muted",
        )}
      >
        {state.mic ? <AudioWaveform size={17} /> : <MicOff size={17} />}
      </button>

      <div data-tauri-drag-region className="flex min-w-0 flex-1 flex-col gap-[3px]">
        <input
          ref={input}
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
            if (e.key === "Escape") e.currentTarget.blur();
          }}
          placeholder={prompt}
          aria-label="Command transcript"
          spellCheck={false}
          className={cn(
            "w-full truncate bg-transparent text-ui font-medium outline-none",
            heard ? "placeholder:text-ink" : "placeholder:text-bar-muted",
          )}
        />
        <div data-tauri-drag-region className="flex items-center gap-[5px] text-micro text-bar-muted">
          <span
            className={cn(
              "size-1 shrink-0 rounded-full",
              state.busy ? "bg-bar-busy" : state.mic ? "bg-bar-accent" : "bg-bar-muted",
            )}
          />
          <span data-tauri-drag-region className="truncate">
            {barStatus(state, step)}
          </span>
          {state.queued > 0 ? <span className="shrink-0">· {state.queued} queued</span> : null}
        </div>
      </div>

      {hasTyped ? (
        <button
          type="button"
          onClick={submit}
          aria-label="Run command"
          title="Run command · Return"
          className="flex size-[27px] shrink-0 items-center justify-center rounded-full bg-bar-accent text-bar-on-accent"
        >
          <ArrowUp size={12} strokeWidth={3} />
        </button>
      ) : null}

      {state.busy ? (
        <button
          type="button"
          onClick={() => void commands.listenCancel()}
          aria-label="Stop task"
          className="flex h-7 w-[26px] shrink-0 items-center justify-center text-bar-muted"
        >
          <Square size={10} fill="currentColor" />
        </button>
      ) : null}
    </div>
  );
}
