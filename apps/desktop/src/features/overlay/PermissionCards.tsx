import { useEffect, useState } from "react";
import type { AgentPermission, ApprovalReply } from "@/lib/bindings";
import { diffLines } from "@/lib/diff";
import { AGENT_NAME } from "./agentIdentity";

/** The input keys worth leading with, per tool — the headline before the rest. */
const HEADLINE_ARG: Record<string, string> = {
  Bash: "command",
  Read: "file_path",
  Write: "file_path",
  Edit: "file_path",
  Glob: "pattern",
  Grep: "pattern",
};

/** The file name from a path — a card wants the leaf, not the full path. */
export function baseName(path: string): string {
  return path.split(/[/\\]/).pop() || path;
}

/**
 * A plain-words verb phrase for the header: `run a command`, `edit overlay.tsx`.
 * `object` is the file name where a tool acts on a file — shown after the bold
 * action so the header reads as a sentence.
 */
function APPROVAL_VERB(
  tool: string,
  headline: string | undefined,
): { action: string; object?: string } {
  switch (tool) {
    case "Bash":
      return { action: "run a command" };
    case "Edit":
    case "NotebookEdit":
      return { action: "edit", object: headline ? baseName(headline) : "a file" };
    case "Write":
      return { action: "write", object: headline ? baseName(headline) : "a file" };
    case "Read":
      return { action: "read", object: headline ? baseName(headline) : "a file" };
    default:
      return { action: `use ${tool}` };
  }
}

/**
 * The Approve / Deny card. The agent is blocked on this — the socket thread in
 * `sessions.rs` is parked until `onResolve` sends the answer back.
 *
 * The whole tool call is shown, not a summary: the headline argument first
 * (a command, a path), then the rest, then the cwd — the point of a gate is
 * that the user can see exactly what would run.
 */
/**
 * One pending request: the model asking a question gets the question card,
 * anything else gets the approval card. Both resolve through the same
 * command — a question's answers are just a richer reply.
 */
export function PermissionCard({
  request,
  queued,
  onResolve,
}: {
  request: AgentPermission;
  queued: number;
  onResolve: (id: string, reply: ApprovalReply) => void;
}) {
  return request.questions.length > 0 ? (
    <QuestionCard request={request} queued={queued} onResolve={onResolve} />
  ) : (
    <ApprovalCard request={request} queued={queued} onResolve={onResolve} />
  );
}

/**
 * The permission prompt, with the terminal's own three options — the point is
 * that answering here reads exactly like answering in the terminal would.
 */
function ApprovalCard({
  request,
  queued,
  onResolve,
}: {
  request: AgentPermission;
  queued: number;
  onResolve: (id: string, reply: ApprovalReply) => void;
}) {
  const headlineKey = HEADLINE_ARG[request.tool];
  const headline = headlineKey ? request.input[headlineKey] : undefined;
  // An edit's before/after renders as a real diff, not two walls of text
  // under `old_string` / `new_string` keys. Neither reference app does this —
  // open-vibe-island decodes the strings and discards them — but the card
  // already carries them whole, so showing the actual change is nearly free.
  const change =
    request.tool === "Edit" && request.input.old_string !== undefined
      ? diffLines(request.input.old_string, request.input.new_string ?? "")
      : request.tool === "Write" && request.input.content !== undefined
        ? diffLines("", request.input.content)
        : null;
  const DIFFED = ["old_string", "new_string", "content"];
  const rest = Object.entries(request.input).filter(
    ([k]) => k !== headlineKey && (change === null || !DIFFED.includes(k)),
  );
  // The third option opens a message box instead of denying outright — the
  // "tell Claude what to do differently" the terminal offers.
  const [denying, setDenying] = useState(false);
  const [message, setMessage] = useState("");

  // Enter allows, Escape denies — the two things a hand on the keyboard
  // reaches for. Scoped to the card so it does not fight the panel's own
  // Escape. While the message box is open, Enter belongs to it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" && !denying) onResolve(request.id, { kind: "allowOnce" });
      if (e.key === "Escape") {
        e.stopPropagation();
        onResolve(request.id, { kind: "deny", message: null });
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [request.id, onResolve, denying]);

  // Danger is judged in Rust (where the command is parsed) and rides the wire.
  const danger = request.danger;
  // A verb phrase in plain words — "run a command", "edit overlay.tsx" — so the
  // header reads at a glance instead of naming the raw tool.
  const verb = APPROVAL_VERB(request.tool, headline);

  return (
    <div className="perm" data-danger={danger || undefined}>
      <span className="perm__rule" aria-hidden />
      <div className="perm__inner">
        <div className="perm__head">
          <span className="perm__verb">
            Wants to <b>{verb.action}</b>
            {verb.object && <> {verb.object}</>}
          </span>
          {queued > 0 && <span className="perm__queue">+{queued}</span>}
          <span className="perm__who">{AGENT_NAME[request.agent] ?? request.agent}</span>
        </div>

        {/* An edit shows its diff; anything else shows the exact target. */}
        {change ? (
          <div className="diff">
            {change.map((line, i) => (
              <div className="diff__line" data-kind={line.kind} key={i}>
                <span className="diff__sign" aria-hidden>
                  {line.kind === "add" ? "+" : line.kind === "del" ? "−" : " "}
                </span>
                {line.text || " "}
              </div>
            ))}
          </div>
        ) : (
          headline && <code className="perm__target">{headline}</code>
        )}

        {rest.length > 0 && (
          <dl className="perm__args">
            {rest.map(([key, value]) => (
              <div className="perm__arg" key={key}>
                <dt>{key}</dt>
                <dd>{value}</dd>
              </div>
            ))}
          </dl>
        )}

        {request.cwd && !change && <p className="perm__path">{request.cwd}</p>}

        {denying ? (
          <form
            className="perm__deny-form"
            onSubmit={(e) => {
              e.preventDefault();
              onResolve(request.id, { kind: "deny", message: message.trim() || null });
            }}
          >
            <input
              className="perm__deny-input"
              autoFocus
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              placeholder="Tell Claude what to do differently…"
            />
            <button className="btn btn--deny" type="submit">
              Send
            </button>
          </form>
        ) : (
          <div className="perm__acts">
            <button
              className="btn btn--allow"
              onClick={() => onResolve(request.id, { kind: "allowOnce" })}
            >
              Allow <span className="btn__k">⏎</span>
            </button>
            {request.tool && (
              <button
                className="btn btn--always"
                onClick={() => onResolve(request.id, { kind: "allowAlways" })}
              >
                Always allow {request.tool}
              </button>
            )}
            <button className="btn btn--deny" onClick={() => setDenying(true)}>
              Deny <span className="btn__k">esc</span>
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * The label the CLI gives its injected freeform choice; not in the payload,
 * so it cannot collide with a real option of the same name — a real "Other"
 * from the model would simply merge with it, which is what the CLI does too.
 */
const OTHER = "Other";

/**
 * The model's own multi-option prompt (`AskUserQuestion`), one section per
 * question, mirroring the terminal: options with descriptions, multi-select
 * where the model asked for it, and an injected freeform "Other".
 *
 * Answers resolve as `{ kind: "answer" }` keyed by each question's text, with
 * multi-select labels joined `", "` — the exact shape the agent reads back
 * out of `updatedInput.answers`.
 */
function QuestionCard({
  request,
  queued,
  onResolve,
}: {
  request: AgentPermission;
  queued: number;
  onResolve: (id: string, reply: ApprovalReply) => void;
}) {
  // Selections per question index; `Other` selects the freeform text below.
  const [picked, setPicked] = useState<Record<number, string[]>>({});
  const [other, setOther] = useState<Record<number, string>>({});

  // Escape declines the question the way it declines a permission — the
  // model is told and moves on, rather than waiting a day on a card the user
  // has walked away from.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onResolve(request.id, { kind: "deny", message: null });
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [request.id, onResolve]);

  const toggle = (index: number, label: string, multi: boolean) => {
    setPicked((prev) => {
      const current = prev[index] ?? [];
      const next = multi
        ? current.includes(label)
          ? current.filter((l) => l !== label)
          : [...current, label]
        : [label];
      return { ...prev, [index]: next };
    });
  };

  /** The answer string for one question, or null while it is incomplete. */
  const answerFor = (index: number): string | null => {
    const labels = picked[index] ?? [];
    if (labels.length === 0) return null;
    const resolved = labels.map((label) =>
      label === OTHER ? (other[index] ?? "").trim() : label,
    );
    if (resolved.some((text) => text === "")) return null;
    return resolved.join(", ");
  };

  const complete = request.questions.every((_, i) => answerFor(i) !== null);

  const submit = () => {
    const answers: Record<string, string> = {};
    request.questions.forEach((q, i) => {
      answers[q.question] = answerFor(i) ?? "";
    });
    onResolve(request.id, { kind: "answer", answers });
  };

  // One single-choice question answers on tap — the common case, and the
  // terminal's own feel. Anything richer gets an explicit Answer button.
  const tapSubmits = request.questions.length === 1 && !request.questions[0].multiSelect;

  return (
    <div className="perm">
      <span className="perm__rule" aria-hidden />
      <div className="perm__inner">
        <div className="perm__head">
          <span className="perm__verb">
            Claude has a <b>question</b>
          </span>
          {queued > 0 && <span className="perm__queue">+{queued}</span>}
          <span className="perm__who">{AGENT_NAME[request.agent] ?? request.agent}</span>
        </div>

        {request.questions.map((question, index) => (
        <div className="q" key={index}>
          <span className="q__header">{question.header}</span>
          <p className="q__text">{question.question}</p>
          <div className="q__options">
            {[...question.options, { label: OTHER, description: "" }].map((option) => {
              const selected = (picked[index] ?? []).includes(option.label);
              return (
                <button
                  key={option.label}
                  className="q__option"
                  data-selected={selected || undefined}
                  onClick={() => {
                    if (tapSubmits && option.label !== OTHER) {
                      onResolve(request.id, {
                        kind: "answer",
                        answers: { [question.question]: option.label },
                      });
                      return;
                    }
                    toggle(index, option.label, question.multiSelect);
                  }}
                >
                  <span className="q__label">{option.label}</span>
                  {option.description && (
                    <span className="q__desc">{option.description}</span>
                  )}
                </button>
              );
            })}
          </div>
          {(picked[index] ?? []).includes(OTHER) && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                if (complete) submit();
              }}
            >
              <input
                className="perm__deny-input"
                autoFocus
                value={other[index] ?? ""}
                onChange={(e) => setOther((prev) => ({ ...prev, [index]: e.target.value }))}
                placeholder="Your own answer…"
              />
            </form>
          )}
        </div>
      ))}

        {/* Tap-to-answer needs no button — except when "Other" is picked and
            there is typed text to send. */}
        {(!tapSubmits || (picked[0] ?? []).includes(OTHER)) && (
          <div className="perm__acts">
            <button className="btn btn--allow" disabled={!complete} onClick={submit}>
              Answer
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
