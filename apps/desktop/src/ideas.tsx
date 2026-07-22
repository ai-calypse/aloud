import React, { useEffect, useState, type ReactNode } from "react";
import ReactDOM from "react-dom/client";
import {
  ArrowRightIcon,
  CalendarIcon,
  CheckIcon,
  FileTextIcon,
  MailIcon,
  MousePointer2Icon,
  PlayIcon,
  SearchIcon,
} from "lucide-react";
import { MessageResponse } from "@/components/ai-elements/message";
import { Shimmer } from "@/components/ai-elements/shimmer";
import { Suggestion, Suggestions } from "@/components/ai-elements/suggestion";
import {
  Task,
  TaskContent,
  TaskItem,
  TaskItemFile,
  TaskTrigger,
} from "@/components/ai-elements/task";
import {
  ChainOfThought,
  ChainOfThoughtContent,
  ChainOfThoughtHeader,
  ChainOfThoughtSearchResult,
  ChainOfThoughtSearchResults,
  ChainOfThoughtStep,
} from "@/components/ai-elements/chain-of-thought";
import {
  Plan,
  PlanContent,
  PlanDescription,
  PlanHeader,
  PlanTitle,
} from "@/components/ai-elements/plan";
import { Checkpoint, CheckpointTrigger } from "@/components/ai-elements/checkpoint";
import {
  Queue,
  QueueItem,
  QueueItemContent,
  QueueItemIndicator,
  QueueList,
} from "@/components/ai-elements/queue";
import {
  Confirmation,
  ConfirmationAction,
  ConfirmationActions,
  ConfirmationRequest,
  ConfirmationTitle,
} from "@/components/ai-elements/confirmation";
import {
  Context,
  ContextContent,
  ContextContentBody,
  ContextContentHeader,
  ContextInputUsage,
  ContextOutputUsage,
  ContextTrigger,
} from "@/components/ai-elements/context";
import { Tool, ToolContent, ToolHeader } from "@/components/ai-elements/tool";
import "./index.css";
import "./ideas.css";

/**
 * A scratch page for deciding what the agent should become — dev-only, not a
 * Tauri window and not a build input.
 *
 *     cd apps/desktop && pnpm exec vite --port 5199
 *     open http://localhost:5199/ideas.html
 *
 * Today Aloud's agent runs one spoken command against a couple of connectors.
 * The interesting question is what it should do when there are thirty of them
 * and it can drive the Mac itself, and that question is much easier to answer
 * against something moving than in prose. Every demo below is built from the
 * real AI Elements the app already vendors, drawn in a black panel hanging off
 * a menu bar — because an idea that only reads well in a 900px column is not
 * an idea for this app.
 *
 * The constraints these are all trying to respect, which is what makes them
 * different from the same list on a chat product:
 *
 *   - your hands are busy. You are dictating *into another app*, mid-task.
 *   - the surface is small and glanceable, and it steals focus reluctantly.
 *   - the stakes are real. It has write access to your mail and calendar.
 *   - you are one keypress from talking, so the cheapest reply is a spoken one.
 *
 * None of this is committed to the product. It is a menu.
 */

// ---------------------------------------------------------------------------
// The harness: each demo is a list of frames played on a timer.
// ---------------------------------------------------------------------------

/** Advances through `steps` while `playing`, returning the frame index. */
function useScript(steps: number, playing: boolean, ms = 1100) {
  const [at, setAt] = useState(0);
  useEffect(() => {
    if (!playing) return;
    setAt(0);
    let i = 0;
    const timer = setInterval(() => {
      i += 1;
      setAt(i);
      if (i >= steps - 1) clearInterval(timer);
    }, ms);
    return () => clearInterval(timer);
  }, [playing, steps, ms]);
  return at;
}

function Stage({
  heard,
  children,
  onPlay,
}: {
  heard?: string;
  children: ReactNode;
  onPlay: () => void;
}) {
  return (
    <div className="stage">
      <div className="stage__menubar">
        <span style={{ flex: 1 }}>Mail Edit View Window</span>
        <span className="stage__notch" />
        <span style={{ flex: 1, textAlign: "right" }}>Wed 09:41</span>
      </div>
      <div className="stage__body">
        <div className="panel">
          {heard && <p className="panel__heard">{heard}</p>}
          <div className="panel__stack">{children}</div>
        </div>
      </div>
      <button className="play" onClick={onPlay}>
        <PlayIcon size={13} /> Play
      </button>
    </div>
  );
}

function Idea({
  no,
  title,
  what,
  why,
  elements,
  children,
}: {
  no: number;
  title: string;
  what: string;
  why: string;
  /** `true` marks an element the app does not vendor yet. */
  elements: [string, boolean?][];
  children: ReactNode;
}) {
  return (
    <section className="idea">
      <div>
        <div className="idea__no">{String(no).padStart(2, "0")}</div>
        <h2>{title}</h2>
        <p className="idea__what">{what}</p>
        <p className="idea__why">{why}</p>
        <div className="chips">
          {elements.map(([name, isNew]) => (
            <span key={name} className={`chip${isNew ? " chip--new" : ""}`}>
              {name}
            </span>
          ))}
        </div>
      </div>
      {children}
    </section>
  );
}

/** Wraps a frame so each step of a script fades up rather than snapping in. */
const F = ({ children }: { children: ReactNode }) => (
  <div className="frame">{children}</div>
);

// ---------------------------------------------------------------------------
// 01 — Approve the plan, not every write
// ---------------------------------------------------------------------------

function ApproveThePlan() {
  const [playing, setPlaying] = useState(false);
  const at = useScript(4, playing);
  return (
    <Stage
      heard="Reply to Sam, move standup to 10, and tell the team"
      onPlay={() => setPlaying((p) => !p)}
    >
      {at === 0 && (
        <F>
          <p className="line">
            <Shimmer as="span">Working out what that takes…</Shimmer>
          </p>
        </F>
      )}
      {at >= 1 && (
        <F>
          <Plan defaultOpen>
            <PlanHeader>
              <PlanTitle>Three actions, two of them writes</PlanTitle>
              <PlanDescription>Approve once and it runs straight through.</PlanDescription>
            </PlanHeader>
            <PlanContent>
              <Queue>
                <QueueList>
                  {[
                    "Draft a reply to Sam",
                    "Move standup to 10:00",
                    "Message #team",
                  ].map((t, i) => (
                    <QueueItem key={t}>
                      <span className="flex items-start gap-2">
                        <QueueItemIndicator completed={at >= 3 || (at >= 2 && i === 0)} />
                        <QueueItemContent completed={at >= 3 || (at >= 2 && i === 0)}>
                          {t}
                        </QueueItemContent>
                      </span>
                    </QueueItem>
                  ))}
                </QueueList>
              </Queue>
            </PlanContent>
          </Plan>
        </F>
      )}
      {at === 1 && (
        <F>
          <Confirmation approval={{ id: "plan" }} state="approval-requested">
            <ConfirmationTitle>Two of these write. Run all three?</ConfirmationTitle>
            <ConfirmationRequest>
              <ConfirmationActions>
                <ConfirmationAction variant="outline">Let me pick</ConfirmationAction>
                <ConfirmationAction>Run all</ConfirmationAction>
              </ConfirmationActions>
            </ConfirmationRequest>
          </Confirmation>
        </F>
      )}
      {at >= 3 && (
        <F>
          <MessageResponse>Done — reply drafted, standup moved, team told.</MessageResponse>
        </F>
      )}
    </Stage>
  );
}

// ---------------------------------------------------------------------------
// 02 — Point at it and ask
// ---------------------------------------------------------------------------

function PointAndAsk() {
  const [playing, setPlaying] = useState(false);
  const at = useScript(3, playing);
  return (
    <Stage heard="What is this?" onPlay={() => setPlaying((p) => !p)}>
      <F>
        <div className="pointer">
          {at >= 1 && (
            <>
              <span className="pointer__crop" />
              <span className="pointer__label">Xcode › Build log</span>
            </>
          )}
        </div>
      </F>
      {at === 1 && (
        <F>
          <p className="line">
            <MousePointer2Icon size={15} />
            <Shimmer as="span">Reading what is under your pointer…</Shimmer>
          </p>
        </F>
      )}
      {at >= 2 && (
        <F>
          <MessageResponse>
            {
              "That is a **linker** error, not a compile one. `_swift_FORCE_LOAD` means a framework was found at build time but not at link time — usually a target missing from *Link Binary With Libraries*."
            }
          </MessageResponse>
        </F>
      )}
    </Stage>
  );
}

// ---------------------------------------------------------------------------
// 03 — Answer it without touching the keyboard
// ---------------------------------------------------------------------------

function AnswerBySpeaking() {
  const [playing, setPlaying] = useState(false);
  const at = useScript(3, playing);
  return (
    <Stage heard="Put lunch with Priya in on Thursday" onPlay={() => setPlaying((p) => !p)}>
      {at >= 1 && (
        <F>
          <p className="muted">You have two calendars. Which one?</p>
          <Suggestions>
            <Suggestion suggestion="Personal" />
            <Suggestion suggestion="Work" />
            <Suggestion suggestion="Ask me every time" />
          </Suggestions>
        </F>
      )}
      {at >= 2 && (
        <F>
          <MessageResponse>Added to **Personal** — Thursday 13:00, lunch with Priya.</MessageResponse>
        </F>
      )}
      {at === 0 && (
        <F>
          <p className="line">
            <Shimmer as="span">Looking through your calendar…</Shimmer>
          </p>
        </F>
      )}
    </Stage>
  );
}

// ---------------------------------------------------------------------------
// 04 — Take it back
// ---------------------------------------------------------------------------

function TakeItBack() {
  const [playing, setPlaying] = useState(false);
  const at = useScript(3, playing);
  return (
    <Stage heard="Send Sam the notes" onPlay={() => setPlaying((p) => !p)}>
      {at >= 1 && (
        <F>
          <Tool defaultOpen={false}>
            <ToolHeader
              type="tool-GMAIL_SEND_EMAIL"
              state="output-available"
              title="Sent an email to sam@…"
            />
            <ToolContent />
          </Tool>
        </F>
      )}
      {at === 1 && (
        <F>
          <Checkpoint>
            <CheckpointTrigger>Undo — 8s</CheckpointTrigger>
          </Checkpoint>
        </F>
      )}
      {at >= 2 && (
        <F>
          <MessageResponse>Pulled back before it left your outbox.</MessageResponse>
        </F>
      )}
    </Stage>
  );
}

// ---------------------------------------------------------------------------
// 05 — What you might say next
// ---------------------------------------------------------------------------

function SayNext() {
  const [playing, setPlaying] = useState(false);
  const at = useScript(2, playing);
  return (
    <Stage heard="What are my events today?" onPlay={() => setPlaying((p) => !p)}>
      <F>
        <MessageResponse>
          {
            "Three: **standup** at 09:30, a **design review** at 11, and **lunch with Priya** at 13:00. Nothing after 15:00."
          }
        </MessageResponse>
      </F>
      {at >= 1 && (
        <F>
          <Suggestions>
            <Suggestion suggestion="Move standup to 10" />
            <Suggestion suggestion="What's the design review about?" />
            <Suggestion suggestion="Block the afternoon" />
          </Suggestions>
        </F>
      )}
    </Stage>
  );
}

// ---------------------------------------------------------------------------
// 06 — Think harder, and show it
// ---------------------------------------------------------------------------

function ThinkHarder() {
  const [playing, setPlaying] = useState(false);
  const at = useScript(4, playing);
  const status = (i: number) => (at > i ? "complete" : at === i ? "active" : "pending");
  return (
    <Stage
      heard="Find a 90-minute slot all three of us can do before Friday"
      onPlay={() => setPlaying((p) => !p)}
    >
      <F>
        <ChainOfThought defaultOpen>
          <ChainOfThoughtHeader>Working it out</ChainOfThoughtHeader>
          <ChainOfThoughtContent>
            <ChainOfThoughtStep
              icon={CalendarIcon}
              label="Reading three calendars"
              status={status(0)}
            />
            <ChainOfThoughtStep
              icon={SearchIcon}
              label="Overlapping the free time"
              status={status(1)}
            >
              {at >= 1 && (
                <ChainOfThoughtSearchResults>
                  <ChainOfThoughtSearchResult>Thu 14:00–16:00</ChainOfThoughtSearchResult>
                  <ChainOfThoughtSearchResult>Fri 09:00–10:30</ChainOfThoughtSearchResult>
                </ChainOfThoughtSearchResults>
              )}
            </ChainOfThoughtStep>
            <ChainOfThoughtStep
              icon={CheckIcon}
              label="Ruling out the one that clashes with your commute"
              status={status(2)}
            />
          </ChainOfThoughtContent>
        </ChainOfThought>
      </F>
      {at >= 3 && (
        <F>
          <MessageResponse>**Thursday 14:00** is the only one that works for all three.</MessageResponse>
        </F>
      )}
    </Stage>
  );
}

// ---------------------------------------------------------------------------
// 07 — What this run is costing
// ---------------------------------------------------------------------------

function RunCost() {
  const [playing, setPlaying] = useState(false);
  const at = useScript(3, playing);
  const used = [1200, 8400, 21500][Math.min(at, 2)];
  return (
    <Stage heard="Go through last week's threads and summarise what I owe people" onPlay={() => setPlaying((p) => !p)}>
      <F>
        <p className="line">
          <Shimmer as="span">{`Round ${Math.min(at + 1, 3)} of 5 — reading threads…`}</Shimmer>
        </p>
      </F>
      <F>
        <Context
          usedTokens={used}
          maxTokens={128000}
          modelId="openai/gpt-oss-20b"
          usage={{
            inputTokens: used,
            outputTokens: 320,
            totalTokens: used + 320,
            inputTokenDetails: {
              noCacheTokens: used,
              cacheReadTokens: 0,
              cacheWriteTokens: 0,
            },
            outputTokenDetails: { textTokens: 320, reasoningTokens: 0 },
          }}
        >
          <ContextTrigger />
          <ContextContent>
            <ContextContentHeader />
            <ContextContentBody>
              <ContextInputUsage />
              <ContextOutputUsage />
            </ContextContentBody>
          </ContextContent>
        </Context>
      </F>
    </Stage>
  );
}

// ---------------------------------------------------------------------------
// 08 — Turn it into a verb
// ---------------------------------------------------------------------------

function MakeItAVerb() {
  const [playing, setPlaying] = useState(false);
  const at = useScript(3, playing);
  return (
    <Stage heard="Open Zoom, mute me, and pull up the standup notes" onPlay={() => setPlaying((p) => !p)}>
      <F>
        <Task defaultOpen={at < 2}>
          <TaskTrigger title={at >= 2 ? "Standup setup" : "Did three things"} />
          <TaskContent>
            <TaskItem>Opened Zoom and joined the recurring room</TaskItem>
            <TaskItem>Muted your microphone</TaskItem>
            <TaskItem>
              Opened <TaskItemFile>standup.md</TaskItemFile>
            </TaskItem>
          </TaskContent>
        </Task>
      </F>
      {at === 1 && (
        <F>
          <p className="muted">Third Monday running. Want this as one command?</p>
          <Suggestions>
            <Suggestion suggestion="Save as “standup setup”" />
            <Suggestion suggestion="No thanks" />
          </Suggestions>
        </F>
      )}
      {at >= 2 && (
        <F>
          <p className="line">
            <CheckIcon size={15} /> Saved. Say <strong>“standup setup”</strong> next Monday.
          </p>
        </F>
      )}
    </Stage>
  );
}

// ---------------------------------------------------------------------------
// 09 — Keep talking while it works
// ---------------------------------------------------------------------------

function KeepTalking() {
  const [playing, setPlaying] = useState(false);
  const at = useScript(4, playing);
  const queued = [
    "…and add it to the standup notes",
    "…then tell Priya it moved",
  ].slice(0, Math.max(0, Math.min(at, 2)));
  return (
    <Stage heard="Move standup to 10" onPlay={() => setPlaying((p) => !p)}>
      <F>
        <Tool defaultOpen={false}>
          <ToolHeader
            type="tool-GOOGLECALENDAR_UPDATE_EVENT"
            state={at >= 3 ? "output-available" : "input-available"}
            title="Moving standup to 10:00"
          />
          <ToolContent />
        </Tool>
      </F>
      {queued.length > 0 && (
        <F>
          <Queue>
            <QueueList>
              {queued.map((q) => (
                <QueueItem key={q}>
                  <span className="flex items-start gap-2">
                    <QueueItemIndicator />
                    <QueueItemContent>{q}</QueueItemContent>
                  </span>
                </QueueItem>
              ))}
            </QueueList>
          </Queue>
        </F>
      )}
    </Stage>
  );
}

// ---------------------------------------------------------------------------
// 10 — Bring what you are looking at
// ---------------------------------------------------------------------------

function BringWhatYouSee() {
  const [playing, setPlaying] = useState(false);
  const at = useScript(3, playing);
  return (
    <Stage heard="Send this to Priya" onPlay={() => setPlaying((p) => !p)}>
      {at >= 1 && (
        <F>
          <Task defaultOpen>
            <TaskTrigger title="Picked up from what you were looking at" />
            <TaskContent>
              <TaskItem>
                <FileTextIcon size={13} /> <TaskItemFile>Q3-release-notes.pdf</TaskItemFile>{" "}
                — frontmost in Preview
              </TaskItem>
              <TaskItem>
                <MailIcon size={13} /> Priya Shah — from the thread behind it
              </TaskItem>
            </TaskContent>
          </Task>
        </F>
      )}
      {at >= 2 && (
        <F>
          <Confirmation approval={{ id: "send" }} state="approval-requested">
            <ConfirmationTitle>Send the PDF to priya@…?</ConfirmationTitle>
            <ConfirmationRequest>
              <ConfirmationActions>
                <ConfirmationAction variant="outline">Pick a different file</ConfirmationAction>
                <ConfirmationAction>Send</ConfirmationAction>
              </ConfirmationActions>
            </ConfirmationRequest>
          </Confirmation>
        </F>
      )}
      {at === 0 && (
        <F>
          <p className="line">
            <Shimmer as="span">Looking at what is in front of you…</Shimmer>
          </p>
        </F>
      )}
    </Stage>
  );
}

// ---------------------------------------------------------------------------

function Ideas() {
  return (
    <div className="wrap">
      <header className="masthead">
        <h1>What the agent could become</h1>
        <p>
          Ten things Aloud’s agent could do once it has thirty connectors and can
          drive the Mac. Each one is built from the real AI Elements the app
          vendors, drawn in a black panel hanging off a menu bar — because an
          idea that only reads well in a wide column is not an idea for this app.
        </p>
        <p>
          They all answer to the same four constraints: your hands are busy, the
          surface is small, the stakes are real, and you are one keypress from
          just saying the answer.
        </p>
        <p className="muted">Blue chips are components not vendored yet. Press Play.</p>
      </header>

      <div className="rule" />

      <Idea
        no={1}
        title="Approve the plan, not every write"
        what="When a command turns into several actions, it shows the whole plan and takes one decision — instead of stopping you at each write."
        why="Today every write blocks on its own confirmation. With thirty connectors a single command could hold four of them, which is four interruptions while you are mid-sentence in another app. One gate is safer too: you approve a shape you can see, not three prompts you have stopped reading."
        elements={[["Plan", true], ["Queue"], ["Confirmation"]]}
      >
        <ApproveThePlan />
      </Idea>

      <Idea
        no={2}
        title="Point at it and ask"
        what="Hold the key, point at something on screen, ask. It reads the region under your pointer and answers — showing you exactly what it looked at."
        why="The fastest possible “what is this error”. No screenshot, no paste, no switching windows. Showing the crop matters as much as the answer: it is how you know it read the right thing, and it is the difference between a tool you trust and one you double-check."
        elements={[["Message"], ["Shimmer"], ["Sources", true]]}
      >
        <PointAndAsk />
      </Idea>

      <Idea
        no={3}
        title="Answer it without touching the keyboard"
        what="When it needs one detail — which calendar, which account — it offers the options instead of asking an open question."
        why="A text field is the wrong question to ask someone who is dictating. Options can be spoken, clicked, or ignored, and they double as a way to say “stop asking me this”. This is the single most common reason a voice command dies halfway."
        elements={[["Suggestion", true]]}
      >
        <AnswerBySpeaking />
      </Idea>

      <Idea
        no={4}
        title="Take it back"
        what="After a write lands, a short window to undo it — pull the mail, revert the event."
        why="Voice misfires in ways typing does not: “send to Sam” heard as “send to Sara” is one phoneme and an unbounded cost. A confirmation gate catches the wrong action; it does not catch the wrong argument, because both look right until you see the result."
        elements={[["Checkpoint", true], ["Tool"]]}
      >
        <TakeItBack />
      </Idea>

      <Idea
        no={5}
        title="What you might say next"
        what="After an answer, two or three follow-ups you can just say."
        why="Keeps you in voice. Without them the second question costs a keypress and a re-think, which is where most sessions end at one command — and the follow-up is usually the valuable one."
        elements={[["Suggestion", true], ["Message"]]}
      >
        <SayNext />
      </Idea>

      <Idea
        no={6}
        title="Think harder, and show it"
        what="Hard multi-step requests escalate to a reasoning model, and you watch it work rather than watching a spinner."
        why="Reasoning is deliberately off — it is a large part of why Ask answers in 618ms. Some questions are worth the trade, but only if the wait is legible. Four seconds of visible reasoning reads as thorough; four seconds of nothing reads as broken."
        elements={[["Chain of Thought", true], ["Reasoning", true]]}
      >
        <ThinkHarder />
      </Idea>

      <Idea
        no={7}
        title="What this run is costing"
        what="For a long run, what it has read and how many rounds it has taken."
        why="The tool loop can go five rounds and today every one of them is invisible. With thirty connectors the tool list alone becomes the expensive part of the prompt, and “why did that take nine seconds” should be answerable from the panel rather than the timings database."
        elements={[["Context", true], ["Shimmer"]]}
      >
        <RunCost />
      </Idea>

      <Idea
        no={8}
        title="Turn it into a verb"
        what="When you have done the same sequence a few times, it offers to name it. Then you say the name."
        why="This is where connectors compound. One command that opens Zoom, mutes you and pulls up the notes is worth more than three that each do one thing — and the app is the only thing in a position to notice it is the third Monday."
        elements={[["Task", true], ["Suggestion", true]]}
      >
        <MakeItAVerb />
      </Idea>

      <Idea
        no={9}
        title="Keep talking while it works"
        what="A second command while the first is still running gets queued instead of colliding with it."
        why="Speaking is faster than executing. A calendar write takes two seconds and the next sentence is already out of your mouth — today that fights the run in flight. Queueing turns the mismatch into the feature: you talk at your speed and it works at its own."
        elements={[["Queue"], ["Tool"]]}
      >
        <KeepTalking />
      </Idea>

      <Idea
        no={10}
        title="Bring what you are looking at"
        what="“Send this to Priya” — it works out what *this* is from what is in front of you, and what it picked up is part of the confirmation."
        why="The most natural sentence in voice is the one full of pronouns, and it is the one a text box cannot serve. The risk is that a wrong guess is invisible, so the guess has to be the thing you approve — not a detail behind a disclosure triangle."
        elements={[["Task", true], ["Attachments", true], ["Confirmation"]]}
      >
        <BringWhatYouSee />
      </Idea>

      <p className="footnote">
        Not a roadmap. If any of these are worth building, 01, 03 and 04 are the
        ones that pay off before the connector count grows — they make the agent
        safer and less likely to stall on the commands it already handles.
        <br />
        <ArrowRightIcon size={13} style={{ verticalAlign: "-2px" }} /> 09 is
        already half-built: the panel now shows the queue the tool loop was
        discarding.
      </p>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Ideas />
  </React.StrictMode>,
);
